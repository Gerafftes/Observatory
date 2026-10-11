"""Validate the UART boundary around one deliberately requested ROM transition."""
lazy import hashlib
lazy import re


RESET = re.compile(rb"\brst:0x")
EXPECTED_RESET = re.compile(
    rb"rst:0x15 \(USB_UART_CHIP_RESET\),boot:0x0 \(DOWNLOAD\(USB/UART0\)\)\r?\n"
)
FIRMWARE = re.compile(rb"TX_BOOT\s+(?:APP_MAIN_ENTER|IDENTITY|BOOT_OK)")


def checked_prefix(raw, prefix, minimum=0):
    size = prefix["size"]
    if (type(size) is not int or not minimum <= size <= len(raw)
            or hashlib.sha256(raw[:size]).hexdigest() != prefix["sha256"]):
        raise ValueError("continuous UART prefix changed or out of order")
    return size


def inspect_held_rom(segment, failure_markers, *, require_reset=False):
    """Permit only the known USB download reset, never a later firmware start."""
    if FIRMWARE.search(segment) or failure_markers(segment):
        raise ValueError("unexpected firmware activity in held ROM segment")
    markers = list(RESET.finditer(segment))
    if len(markers) > 1 or (require_reset and len(markers) != 1):
        raise ValueError("unexpected number of held ROM reset markers")
    if markers:
        header = EXPECTED_RESET.match(segment, markers[0].start())
        if header is None or b"waiting for download" not in segment[header.end():]:
            raise ValueError("unexpected reset kind or missing ROM download confirmation")


def inspect_after_window(raw, snapshot, rom_prefix, controlled_prefix,
                         inspect_initial, failure_markers):
    """Keep the strict pre-ROM guard; validate the separately bounded ROM bytes."""
    if raw[:len(snapshot)] != snapshot:
        raise ValueError("continuous pre-interruption UART prefix changed")
    end = (len(raw) if controlled_prefix is None
           else checked_prefix(raw, controlled_prefix, len(snapshot)))
    begin = snapshot.rfind(b"\n") + 1
    if rom_prefix is None:
        inspect_initial(raw[begin:end])
        return
    boundary = checked_prefix(raw, rom_prefix, len(snapshot))
    if boundary > end:
        raise ValueError("ROM handoff follows the controlled boot boundary")
    inspect_initial(raw[begin:boundary])
    if FIRMWARE.search(raw[begin:boundary]):
        raise ValueError("unexpected firmware start before ROM handoff")
    # Include the preceding line when checking for a marker split by the handoff.
    line_start = raw.rfind(b"\n", begin, boundary) + 1
    overlap = raw[max(begin, line_start):boundary]
    if failure_markers(overlap + raw[boundary:end]):
        raise ValueError("firmware failure across or after ROM handoff")
    if overlap and (RESET.search(overlap + raw[boundary:end])
                    or FIRMWARE.search(overlap + raw[boundary:end])):
        first = re.search(rb"\brst:0x|TX_BOOT\s+(?:APP_MAIN_ENTER|IDENTITY|BOOT_OK)",
                          overlap + raw[boundary:end])
        if first.start() < len(overlap):
            raise ValueError("reset or firmware marker crosses ROM handoff boundary")
    inspect_held_rom(raw[boundary:end], failure_markers)
