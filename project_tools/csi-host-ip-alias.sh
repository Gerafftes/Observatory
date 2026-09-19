#!/usr/bin/env zsh
set -e

IFACE="${1:-en0}"
CSI_HOST_IP="${2:-192.168.4.50}"
NETMASK="${3:-255.255.255.0}"

echo "CSI interface: $IFACE"
echo "Desired CSI host alias: $CSI_HOST_IP"

if ifconfig "$IFACE" | grep -q "inet $CSI_HOST_IP "; then
  echo "OK: $CSI_HOST_IP is already assigned on $IFACE"
else
  if ping -c 1 -W 500 "$CSI_HOST_IP" >/dev/null 2>&1; then
    echo "ERROR: $CSI_HOST_IP already responds on the network, but it is not assigned to $IFACE."
    echo "Do not add this alias now; another device may have taken the intended host IP."
    echo "Fix: reconnect devices or choose a different fixed host IP and reprovision RX nodes once."
    exit 2
  fi
  echo "Adding alias $CSI_HOST_IP/$NETMASK to $IFACE"
  sudo ifconfig "$IFACE" alias "$CSI_HOST_IP" "$NETMASK"
fi

echo
echo "Current $IFACE IPv4 addresses:"
ifconfig "$IFACE" | grep "inet 192.168.4" || true

echo
echo "If you need to remove the alias later:"
echo "sudo ifconfig $IFACE -alias $CSI_HOST_IP"
