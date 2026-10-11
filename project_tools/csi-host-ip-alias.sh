#!/bin/zsh

set -u

readonly LABEL="com.ruview.csi-host-ip-alias"
readonly TARGET_IP="192.168.4.50"
readonly GATEWAY_IP="192.168.4.1"
readonly EXPECTED_NETMASK="255.255.255.0"
readonly EXPECTED_TX_BSSID="ae:27:6e:a8:d2:64"
readonly INSTALL_DIR="/Library/Application Support/RuView"
readonly INSTALL_PATH="$INSTALL_DIR/csi-host-ip-alias.sh"
readonly PLIST_PATH="/Library/LaunchDaemons/$LABEL.plist"
readonly STATE_FILE="/var/run/$LABEL.state"
readonly STATUS_FILE="/var/run/$LABEL.status"
readonly CONFLICT_FILE="/var/run/$LABEL.conflict"
readonly MONITOR_PID_FILE="/var/run/$LABEL.tcpdump.pid"
readonly HOLD_FILE="/var/run/$LABEL.conflict-hold"
readonly LOG_PATH="/var/log/$LABEL.log"
readonly ERROR_LOG_PATH="/var/log/$LABEL.error.log"
readonly SOURCE_PATH="${0:A}"
readonly POLL_SECONDS=3
readonly CONFLICT_HOLD_SECONDS=60

MONITOR_PID=""

usage() {
  /bin/cat <<EOF
Usage: ${SOURCE_PATH:t} COMMAND

  install     Install and start the LaunchDaemon (sudo required)
  status      Show DHCP, TX identity, alias, and daemon state
  disable     Stop it and remove only its owned /32 alias (sudo required)
  enable      Start an installed daemon (sudo required)
  uninstall   Disable it and remove all installed files (sudo required)
  self-test   Test parsing and validation without changing the network

macOS stays configured for DHCP. The daemon adds only $TARGET_IP/32 and only
when the TX BSSID, gateway, DHCP server, and subnet all match.
EOF
}

require_root() {
  (( EUID == 0 )) && return 0
  print -u2 -- "ERROR: '$1' requires sudo."
  return 1
}

valid_interface() {
  local name="${1:-}"
  [[ -n "$name" && "$name" != *[^A-Za-z0-9_.-]* ]]
}

valid_mac() {
  local mac="${1:l}"
  [[ "$mac" =~ '^[0-9a-f]{2}(:[0-9a-f]{2}){5}$' ]]
}

packet_value() {
  local packet="$1"
  local key="$2"
  print -r -- "$packet" | /usr/bin/awk -v key="$key" '
    {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      if (index(line, key) != 1) next
      rest = substr(line, length(key) + 1)
      if (rest !~ /^[[:space:]]*(\([^)]*\))?[[:space:]]*[:=]/) next
      sub(/^[[:space:]]*(\([^)]*\))?[[:space:]]*[:=][[:space:]]*/, "", rest)
      gsub(/[{}]/, "", rest)
      split(rest, values, /[,[:space:]]+/)
      print values[1]
      exit
    }
  '
}

arp_mac() {
  print -r -- "$1" | /usr/bin/awk '
    {
      for (i = 1; i <= NF; i++) {
        if ($i == "at") { print tolower($(i + 1)); exit }
      }
    }
  '
}

route_interface() {
  /sbin/route -n get "$GATEWAY_IP" 2>/dev/null |
    /usr/bin/awk '$1 == "interface:" { print $2; exit }'
}

interface_mac() {
  /sbin/ifconfig "$1" 2>/dev/null |
    /usr/bin/awk '$1 == "ether" { print tolower($2); exit }'
}

has_address() {
  /sbin/ifconfig "$1" 2>/dev/null | /usr/bin/awk -v ip="$2" '
    $1 == "inet" && $2 == ip { found = 1 }
    END { exit(found ? 0 : 1) }
  '
}

has_owned_alias() {
  /sbin/ifconfig "$1" 2>/dev/null | /usr/bin/awk -v ip="$TARGET_IP" '
    $1 == "inet" && $2 == ip {
      for (i = 3; i <= NF; i++) {
        if ($i == "netmask" && $(i + 1) == "0xffffffff") found = 1
      }
    }
    END { exit(found ? 0 : 1) }
  '
}

target_interface() {
  /sbin/ifconfig -a 2>/dev/null | /usr/bin/awk -v ip="$TARGET_IP" '
    /^[[:alnum:]][^:]*:/ { iface = $1; sub(/:$/, "", iface) }
    $1 == "inet" && $2 == ip { print iface; exit }
  '
}

state_value() {
  [[ -r "$STATE_FILE" ]] || return 1
  /usr/bin/awk -F= -v key="$1" '
    $1 == key { sub(/^[^=]*=/, "", $0); print; exit }
  ' "$STATE_FILE"
}

write_state() {
  local iface="$1"
  local signature="$2"
  local temporary="$STATE_FILE.$$"
  umask 077
  /usr/bin/printf '%s\n' \
    "owner=$LABEL" "interface=$iface" "address=$TARGET_IP" \
    "prefix=32" "network=$signature" >| "$temporary" || return 1
  /usr/bin/install -o root -g wheel -m 0600 "$temporary" "$STATE_FILE" || return 1
  /bin/rm -f "$temporary"
}

log_message() {
  local message="$1"
  print -r -- "$(/bin/date -u '+%Y-%m-%dT%H:%M:%SZ') $message"
  /usr/bin/logger -t "$LABEL" -- "$message" 2>/dev/null || true
}

set_status() {
  local next="$1"
  local previous=""
  [[ -r "$STATUS_FILE" ]] && previous="$(<"$STATUS_FILE")"
  [[ "$previous" == "$next" ]] && return 0
  umask 077
  print -r -- "$next" >| "$STATUS_FILE" || return 1
  /usr/sbin/chown root:wheel "$STATUS_FILE" 2>/dev/null || true
  /bin/chmod 0600 "$STATUS_FILE" 2>/dev/null || true
  log_message "$next"
}

gateway_mac() {
  local iface="$1"
  local source_ip="$2"
  local output
  /sbin/ping -n -b "$iface" -S "$source_ip" -c 1 -W 350 \
    "$GATEWAY_IP" >/dev/null 2>&1 || true
  output="$(/usr/sbin/arp -n -i "$iface" "$GATEWAY_IP" 2>/dev/null || true)"
  arp_mac "$output"
}

detect_tx_network() {
  local iface packet op message_type offered_ip local_ip subnet router server
  local interface_status observed_bssid own_mac
  iface="$(route_interface)"
  valid_interface "$iface" || return 1

  interface_status="$(/sbin/ifconfig "$iface" 2>/dev/null |
    /usr/bin/awk '$1 == "status:" { print $2; exit }')"
  [[ "$interface_status" == "active" ]] || return 1
  packet="$(/usr/sbin/ipconfig getpacket "$iface" 2>/dev/null || true)"
  [[ -n "$packet" ]] || return 1

  op="$(packet_value "$packet" op)"
  message_type="$(packet_value "$packet" dhcp_message_type)"
  offered_ip="$(packet_value "$packet" yiaddr)"
  subnet="$(packet_value "$packet" subnet_mask)"
  router="$(packet_value "$packet" router)"
  server="$(packet_value "$packet" server_identifier)"
  local_ip="$(/usr/sbin/ipconfig getifaddr "$iface" 2>/dev/null || true)"

  [[ "$op" == "BOOTREPLY" && "$message_type" == "ACK" ]] || return 1
  [[ "$subnet" == "$EXPECTED_NETMASK" ]] || return 1
  [[ "$router" == "$GATEWAY_IP" && "$server" == "$GATEWAY_IP" ]] || return 1
  [[ "$local_ip" == 192.168.4.* && "$local_ip" != "$TARGET_IP" ]] || return 1
  [[ -z "$offered_ip" || "$offered_ip" == "0.0.0.0" || "$offered_ip" == "$local_ip" ]] || return 1
  [[ "$(route_interface)" == "$iface" ]] || return 1
  has_address "$iface" "$local_ip" || return 1

  observed_bssid="$(gateway_mac "$iface" "$local_ip")"
  valid_mac "$observed_bssid" || return 1
  [[ "$observed_bssid" == "$EXPECTED_TX_BSSID" ]] || return 1
  own_mac="$(interface_mac "$iface")"
  valid_mac "$own_mac" || return 1
  print -r -- "$iface|$local_ip|$observed_bssid|$own_mac"
}

probe_conflict() {
  local iface="$1"
  local source_ip="$2"
  local own_mac="$3"
  local attempt output observed
  has_address "$iface" "$TARGET_IP" && return 2
  /usr/sbin/arp -d "$TARGET_IP" ifscope "$iface" >/dev/null 2>&1 || true
  for attempt in 1 2 3; do
    /sbin/ping -n -b "$iface" -S "$source_ip" -c 1 -W 350 \
      "$TARGET_IP" >/dev/null 2>&1 || true
    output="$(/usr/sbin/arp -n -i "$iface" "$TARGET_IP" 2>/dev/null || true)"
    observed="$(arp_mac "$output")"
    if valid_mac "$observed" && [[ "$observed" != "$own_mac" ]]; then
      print -r -- "$observed"
      return 0
    fi
    /bin/sleep 0.15
  done
  return 1
}

monitor_matches() {
  local pid="$1"
  local details uid command
  [[ "$pid" == <-> ]] || return 1
  details="$(/bin/ps -p "$pid" -o uid= -o command= 2>/dev/null || true)"
  uid="$(print -r -- "$details" | /usr/bin/awk '{ print $1; exit }')"
  command="$(print -r -- "$details" | /usr/bin/awk '{$1=""; sub(/^ /, ""); print; exit}')"
  [[ "$uid" == "0" && "$command" == "/usr/sbin/tcpdump "* && "$command" == *"0xc0a80432"* ]]
}

stop_monitor() {
  local pid="${MONITOR_PID:-}"
  [[ -z "$pid" && -r "$MONITOR_PID_FILE" ]] && pid="$(<"$MONITOR_PID_FILE")"
  if [[ -n "$pid" ]] && monitor_matches "$pid"; then
    /bin/kill "$pid" 2>/dev/null || true
    /bin/sleep 0.1
    /bin/kill -KILL "$pid" 2>/dev/null || true
  fi
  MONITOR_PID=""
  /bin/rm -f "$MONITOR_PID_FILE" "$CONFLICT_FILE"
}

start_monitor() {
  local iface="$1"
  local own_mac="$2"
  local filter
  stop_monitor
  umask 077
  : >| "$CONFLICT_FILE" || return 1
  filter="arp and not ether src $own_mac and (arp[14:4] = 0xc0a80432 or (arp[14:4] = 0 and arp[24:4] = 0xc0a80432))"
  /usr/sbin/tcpdump --immediate-mode -l -nn -e -i "$iface" "$filter" \
    >| "$CONFLICT_FILE" 2>> "$ERROR_LOG_PATH" &
  MONITOR_PID=$!
  print -r -- "$MONITOR_PID" >| "$MONITOR_PID_FILE"
  /usr/sbin/chown root:wheel "$CONFLICT_FILE" "$MONITOR_PID_FILE" 2>/dev/null || true
  /bin/chmod 0600 "$CONFLICT_FILE" "$MONITOR_PID_FILE" 2>/dev/null || true
  /bin/sleep 0.4
  monitor_matches "$MONITOR_PID" || { stop_monitor; return 1; }
}

monitor_active() {
  local pid="${MONITOR_PID:-}"
  [[ -z "$pid" && -r "$MONITOR_PID_FILE" ]] && pid="$(<"$MONITOR_PID_FILE")"
  monitor_matches "$pid"
}

record_conflict() {
  umask 077
  /bin/date +%s >| "$HOLD_FILE"
  /usr/sbin/chown root:wheel "$HOLD_FILE" 2>/dev/null || true
  /bin/chmod 0600 "$HOLD_FILE" 2>/dev/null || true
}

hold_active() {
  local recorded now
  [[ -r "$HOLD_FILE" ]] || return 1
  recorded="$(<"$HOLD_FILE")"
  [[ "$recorded" == <-> ]] || return 1
  now="$(/bin/date +%s)"
  (( now >= recorded && now - recorded < CONFLICT_HOLD_SECONDS ))
}

cleanup_alias() {
  local reason="${1:-cleanup}"
  local owner iface address prefix
  stop_monitor
  [[ -r "$STATE_FILE" ]] || return 0
  owner="$(state_value owner || true)"
  iface="$(state_value interface || true)"
  address="$(state_value address || true)"
  prefix="$(state_value prefix || true)"
  if [[ "$owner" != "$LABEL" || "$address" != "$TARGET_IP" || "$prefix" != "32" ]] ||
    ! valid_interface "$iface"; then
    log_message "Refusing cleanup because ownership state is invalid."
    return 1
  fi
  if has_owned_alias "$iface"; then
    /sbin/ifconfig "$iface" inet "$TARGET_IP" -alias || return 1
    has_owned_alias "$iface" && return 1
    log_message "Removed owned alias $TARGET_IP/32 from $iface ($reason)."
  elif has_address "$iface" "$TARGET_IP"; then
    log_message "Preserved $TARGET_IP on $iface because it is not the owned /32 alias."
  fi
  /bin/rm -f "$STATE_FILE"
}

block_for_conflict() {
  local iface="$1"
  local detail="$2"
  record_conflict
  cleanup_alias "address conflict" || true
  set_status "BLOCKED conflict on $iface: $detail"
}

reconcile() {
  local identity iface dhcp_ip bssid own_mac signature local_target conflict_mac cached_mac
  local -a parts
  identity="$(detect_tx_network)" || {
    cleanup_alias "TX network not recognized" || true
    set_status "INACTIVE TX network not uniquely recognized; DHCP unchanged"
    return
  }
  parts=("${(@s:|:)identity}")
  (( ${#parts[@]} == 4 )) || return 1
  iface="${parts[1]}"
  dhcp_ip="${parts[2]}"
  bssid="${parts[3]}"
  own_mac="${parts[4]}"
  signature="$iface|$bssid|$GATEWAY_IP|$EXPECTED_NETMASK"

  if [[ -r "$STATE_FILE" && "$(state_value interface || true)" != "$iface" ]]; then
    cleanup_alias "interface changed" || { set_status "BLOCKED stale owned alias"; return; }
  fi
  local_target="$(target_interface)"
  if [[ -n "$local_target" ]]; then
    if [[ -r "$STATE_FILE" && "$local_target" == "$iface" ]] && has_owned_alias "$iface"; then
      if [[ -s "$CONFLICT_FILE" ]]; then
        block_for_conflict "$iface" "ARP/DAD monitor"
        return
      fi
      cached_mac="$(arp_mac "$(/usr/sbin/arp -n -i "$iface" "$TARGET_IP" 2>/dev/null || true)")"
      if valid_mac "$cached_mac" && [[ "$cached_mac" != "$own_mac" ]]; then
        block_for_conflict "$iface" "ARP owner $cached_mac"
        return
      fi
      if ! monitor_active && ! start_monitor "$iface" "$own_mac"; then
        cleanup_alias "conflict monitor unavailable" || true
        set_status "BLOCKED passive conflict monitor unavailable; DHCP unchanged"
        return
      fi
      set_status "ACTIVE owned alias $TARGET_IP/32 on $iface; DHCP $dhcp_ip"
      return
    fi
    cleanup_alias "unowned target address" || true
    set_status "BLOCKED $TARGET_IP exists on $local_target without matching ownership"
    return
  fi

  [[ -r "$STATE_FILE" ]] && cleanup_alias "owned alias missing"
  if hold_active; then
    set_status "BLOCKED recent address-conflict hold; DHCP unchanged"
    return
  fi
  conflict_mac="$(probe_conflict "$iface" "$dhcp_ip" "$own_mac")"
  case $? in
    0) block_for_conflict "$iface" "ARP owner $conflict_mac"; return ;;
    2) set_status "BLOCKED target address is already local"; return ;;
  esac
  if ! start_monitor "$iface" "$own_mac"; then
    set_status "BLOCKED passive conflict monitor unavailable; DHCP unchanged"
    return
  fi
  if [[ -s "$CONFLICT_FILE" ]]; then
    block_for_conflict "$iface" "ARP/DAD before activation"
    return
  fi
  write_state "$iface" "$signature" || { stop_monitor; return 1; }
  /sbin/ifconfig "$iface" inet "$TARGET_IP/32" alias || {
    cleanup_alias "activation failed" || true
    set_status "BLOCKED alias activation failed; DHCP unchanged"
    return
  }
  if ! has_owned_alias "$iface"; then
    cleanup_alias "verification failed" || true
    set_status "BLOCKED alias is not configured as /32"
    return
  fi
  [[ -s "$CONFLICT_FILE" ]] && { block_for_conflict "$iface" "ARP/DAD during activation"; return; }
  /bin/rm -f "$HOLD_FILE"
  set_status "ACTIVE owned alias $TARGET_IP/32 on $iface; DHCP $dhcp_ip"
}

daemon_main() {
  require_root daemon || return 1
  trap 'trap - EXIT; cleanup_alias "daemon stopped" || true; exit 0' TERM INT HUP
  trap 'cleanup_alias "daemon exited" || true' EXIT
  stop_monitor
  while true; do
    reconcile || set_status "ERROR reconciliation failed; DHCP unchanged"
    /bin/sleep "$POLL_SECONDS"
  done
}

loaded() {
  /bin/launchctl print "system/$LABEL" >/dev/null 2>&1
}

bootout() {
  loaded || return 0
  /bin/launchctl bootout "system/$LABEL" >/dev/null 2>&1 || true
  for _ in {1..100}; do
    loaded || return 0
    /bin/sleep 0.1
  done
  return 1
}

make_plist() {
  local temporary
  temporary="$(/usr/bin/mktemp "/private/tmp/$LABEL.XXXXXX")" || return 1
  /usr/bin/plutil -create xml1 "$temporary" || return 1
  /usr/bin/plutil -insert Label -string "$LABEL" "$temporary" || return 1
  /usr/bin/plutil -insert ProgramArguments -json "[\"$INSTALL_PATH\",\"daemon\"]" "$temporary" || return 1
  /usr/bin/plutil -insert RunAtLoad -bool true "$temporary" || return 1
  /usr/bin/plutil -insert KeepAlive -bool true "$temporary" || return 1
  /usr/bin/plutil -insert ProcessType -string Background "$temporary" || return 1
  /usr/bin/plutil -insert ThrottleInterval -integer 10 "$temporary" || return 1
  /usr/bin/plutil -insert StandardOutPath -string "$LOG_PATH" "$temporary" || return 1
  /usr/bin/plutil -insert StandardErrorPath -string "$ERROR_LOG_PATH" "$temporary" || return 1
  /usr/bin/plutil -lint "$temporary" >/dev/null || return 1
  /usr/bin/install -o root -g wheel -m 0644 "$temporary" "$PLIST_PATH" || return 1
  /bin/rm -f "$temporary"
}

install_service() {
  require_root install || return 1
  bootout || return 1
  cleanup_alias "reinstall" || return 1
  /bin/mkdir -p "$INSTALL_DIR" || return 1
  /usr/sbin/chown root:wheel "$INSTALL_DIR" || return 1
  /bin/chmod 0755 "$INSTALL_DIR" || return 1
  if [[ "$SOURCE_PATH" != "$INSTALL_PATH" ]]; then
    /usr/bin/install -o root -g wheel -m 0755 "$SOURCE_PATH" "$INSTALL_PATH" || return 1
  fi
  make_plist || return 1
  /bin/launchctl bootstrap system "$PLIST_PATH" || return 1
  /bin/launchctl kickstart -k "system/$LABEL" >/dev/null 2>&1 || true
  print -- "Installed $LABEL. DHCP settings were not changed."
  print -- "Status: sudo '$INSTALL_PATH' status"
  print -- "Remove: sudo '$INSTALL_PATH' uninstall"
}

disable_service() {
  require_root disable || return 1
  bootout || return 1
  cleanup_alias "disabled" || return 1
  /bin/rm -f "$HOLD_FILE"
  set_status "DISABLED owned alias removed; DHCP unchanged"
  print -- "Disabled $LABEL."
}

enable_service() {
  require_root enable || return 1
  [[ -x "$INSTALL_PATH" && -r "$PLIST_PATH" ]] || { print -u2 -- "ERROR: not installed."; return 1; }
  loaded || /bin/launchctl bootstrap system "$PLIST_PATH" || return 1
  /bin/launchctl kickstart -k "system/$LABEL" >/dev/null 2>&1 || true
  print -- "Enabled $LABEL. DHCP settings were not changed."
}

uninstall_service() {
  require_root uninstall || return 1
  bootout || return 1
  cleanup_alias "uninstalled" || return 1
  /bin/rm -f "$PLIST_PATH" "$INSTALL_PATH" "$STATE_FILE" "$STATUS_FILE" \
    "$CONFLICT_FILE" "$MONITOR_PID_FILE" "$HOLD_FILE" "$LOG_PATH" "$ERROR_LOG_PATH"
  /bin/rmdir "$INSTALL_DIR" 2>/dev/null || true
  print -- "Uninstalled $LABEL. IPv4 service settings were not changed."
}

status_main() {
  local identity local_target service_state manager_state
  service_state="stopped"
  loaded && service_state="loaded"
  manager_state="no status recorded"
  [[ -r "$STATUS_FILE" ]] && manager_state="$(<"$STATUS_FILE")"
  print -- "LaunchDaemon: $service_state"
  print -- "Manager: $manager_state"
  identity="$(detect_tx_network)" || true
  if [[ -n "$identity" ]]; then
    local -a parts=("${(@s:|:)identity}")
    print -- "TX network: recognized on ${parts[1]}"
    print -- "DHCP IPv4: ${parts[2]}/24"
    print -- "TX BSSID/gateway MAC: ${parts[3]}"
  else
    print -- "TX network: not uniquely recognized"
  fi
  local_target="$(target_interface)"
  if [[ -n "$local_target" ]] && has_owned_alias "$local_target"; then
    print -- "CSI alias: $TARGET_IP/32 on $local_target"
  elif [[ -n "$local_target" ]]; then
    print -- "CSI address: present on $local_target but not as the managed /32 alias"
  else
    print -- "CSI alias: absent"
  fi
  print -- "Ownership: $([[ -r "$STATE_FILE" ]] && print recorded || print absent)"
  print -- "Conflict monitor: $(monitor_active && print active || print inactive)"
}

self_test() {
  local packet arp failures=0
  packet=$'op = BOOTREPLY\nyiaddr = 192.168.4.2\ndhcp_message_type (uint8): ACK 0x5\nsubnet_mask (ip): 255.255.255.0\nrouter (ip_mult): {192.168.4.1}\nserver_identifier (ip): 192.168.4.1'
  [[ "$(packet_value "$packet" op)" == BOOTREPLY ]] || (( failures++ ))
  [[ "$(packet_value "$packet" dhcp_message_type)" == ACK ]] || (( failures++ ))
  [[ "$(packet_value "$packet" subnet_mask)" == "$EXPECTED_NETMASK" ]] || (( failures++ ))
  [[ "$(packet_value "$packet" router)" == "$GATEWAY_IP" ]] || (( failures++ ))
  [[ "$(packet_value "$packet" server_identifier)" == "$GATEWAY_IP" ]] || (( failures++ ))
  arp='? (192.168.4.1) at AE:27:6E:A8:D2:64 on en0 ifscope [ethernet]'
  [[ "$(arp_mac "$arp")" == "$EXPECTED_TX_BSSID" ]] || (( failures++ ))
  valid_interface en0 || (( failures++ ))
  ! valid_interface 'en0;bad' || (( failures++ ))
  valid_mac "$EXPECTED_TX_BSSID" || (( failures++ ))
  ! valid_mac bad || (( failures++ ))
  (( failures == 0 )) || { print -u2 -- "Self-test failed: $failures checks"; return 1; }
  print -- "Self-test passed: DHCP, ARP, interface, and MAC parsing"
}

main() {
  case "${1:-help}" in
    install) install_service ;;
    daemon) daemon_main ;;
    status) status_main ;;
    disable) disable_service ;;
    enable) enable_service ;;
    uninstall) uninstall_service ;;
    self-test) self_test ;;
    help|-h|--help) usage ;;
    *) print -u2 -- "ERROR: unknown command '${1:-}'."; usage >&2; return 64 ;;
  esac
}

main "$@"
