import sys

sys.path.insert(
    0,
    "/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/lib/python3.9/site-packages",
)

import pcbnew


PCB_PATH = "/Users/Johann/Development/BLL KiCad/mmwave-PCB/PCB-02/PCB-03/PCB-03.kicad_pcb"


def add_zone(board, layer, net_code, points):
    zone = pcbnew.ZONE(board)
    zone.SetLayer(layer)
    zone.SetNetCode(net_code)
    zone.SetLocalClearance(pcbnew.FromMM(0.30))
    zone.SetMinThickness(pcbnew.FromMM(0.25))

    outline = pcbnew.SHAPE_POLY_SET()
    outline.NewOutline()
    for x, y in points:
        outline.Append(pcbnew.VECTOR2I(pcbnew.FromMM(x), pcbnew.FromMM(y)))
    zone.SetOutline(outline)
    board.Add(zone)


def main():
    board = pcbnew.LoadBoard(PCB_PATH)

    # The UART routes are already present in PCB-03.  Keep the operation
    # idempotent so it is safe to rerun while iterating on the plane shape.
    target_layer = pcbnew.F_Cu if len(sys.argv) < 2 or sys.argv[1] == "F.Cu" else pcbnew.B_Cu
    existing_zone_layers = {zone.GetLayer() for zone in board.Zones()}
    net_code = board.GetNetcodeFromNetname("GND")
    points = [
        (149.30, 56.10),
        (175.07, 56.10),
        (175.07, 63.96),
        (171.81, 63.96),
        (171.81, 75.04),
        (175.07, 75.04),
        (175.07, 91.84),
        (149.30, 91.84),
        (149.30, 75.04),
        (154.56, 75.04),
        (154.56, 63.96),
        (149.30, 63.96),
    ]

    if target_layer not in existing_zone_layers:
        add_zone(board, target_layer, net_code, points)

    pcbnew.SaveBoard(PCB_PATH, board)
    print(f"saved {PCB_PATH}; zones={len(board.Zones())}")


if __name__ == "__main__":
    main()
