# KiCad-Projekte

Die bearbeitbaren Quellen liegen gemeinsam mit Software und Dokumentation in
diesem Repository. Für den aktuellen mmWave-Aufbau ist PCB-03 vorgesehen.

| Stand | Projektdatei | Schaltplan | Platine |
| --- | --- | --- | --- |
| PCB-03, aktuelle Revision | [PCB-03.kicad_pro](pcb-03/PCB-03.kicad_pro) | [Schaltplan](pcb-03/PCB-03.kicad_sch) | [Platine](pcb-03/PCB-03.kicad_pcb) |
| PCB-02, frühere Revision | [PCB-02.kicad_pro](pcb-02/PCB-02.kicad_pro) | [Schaltplan](pcb-02/PCB-02.kicad_sch) | [Platine](pcb-02/PCB-02.kicad_pcb) |
| PCB-01, frühere Revision | [PCB-01.kicad_pro](pcb-01/PCB-01.kicad_pro) | [Schaltplan](pcb-01/PCB-01.kicad_sch) | [Platine](pcb-01/PCB-01.kicad_pcb) |
| POC, Entwurfsstand | [POC.kicad_pro](poc/POC.kicad_pro) | [POC](poc/POC.kicad_sch), [POCSchule](poc/POCSchule.kicad_sch) | [Leere POC-Platine](poc/POC.kicad_pcb) |

Öffne die jeweilige `.kicad_pro` mit KiCad 10. Die POC-Platine enthält noch
keinen Aufbau; sie bleibt als ursprünglicher Entwurfsstand erhalten.

Jede PCB-Revision enthält ihre eigene `Project_Footprints.pretty` und eine
`fp-lib-table` mit `${KIPRJMOD}`. Die übrigen Symbole und Footprints stammen
aus den KiCad-Standardbibliotheken. Die drei verwendeten Standard-3D-Modelle
werden über `${KICAD10_3DMODEL_DIR}` aufgelöst; dafür müssen die KiCad-3D-
Bibliotheken installiert sein. Der frühere separate `3D`-Ordner enthielt
keine Modelldateien.

Die Herstellungsdaten und vorhandenen Prüfberichte bleiben bei ihrer Revision.
Sie sind historische Nachweise und ersetzen keine erneute Prüfung vor einer
Fertigung. Automatische lokale Verläufe, Sperrdateien und persönliche
Editorzustände wurden in der unveränderten separaten KiCad-Sicherung bewahrt.

`hardware/mmwave-PCB/PCB-02` ist eine ältere zusätzliche Ablage. Der Einstieg
für weitere Bearbeitungen ist `hardware/pcb-02`.
