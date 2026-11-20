# CSI-Erfassung und Datenqualität

Channel State Information (CSI) beschreibt den Einfluss des Funkkanals auf empfangene WLAN-Signale. Reflexionen, Abschattung und Bewegung verändern die Messung. Ein TX sendet; vier RX beobachten unterschiedliche Funkstrecken. CSI ist Rohmaterial für die Auswertung und für sich genommen noch keine Personenposition.

## Datenweg

1. TX und RX laufen mit einem zum Versuch passenden, dokumentierten Funk- und Firmwarestand.
2. RX erfassen CSI und senden die Daten an den Host. Im getesteten Paarpfad kommen zusätzlich gerichtete RX1↔RX4-Beobachtungen hinzu.
3. Der Server ordnet Quellen und Zeitinformationen zu, prüft Datenqualität und kann Aufnahmen für eine spätere Auswertung speichern.
4. Eine aufbaugebundene Auswertung verwendet geeignete Daten; fehlende oder mehrdeutige Evidenz darf nicht als sichere Position erscheinen.

## Qualität einordnen

Prüfe pro Versuch mindestens Geräte- und Quellbindung, Firmware und Setup, Empfang aller geforderten RX, Datenalter, Zeitbezug, gültige Fenster sowie dokumentierte Lücken und Recorderfehler. Ein Recorder ohne Schreibverluste beweist nicht, dass jedes gesendete Funkpaket angekommen ist. Ebenso ist ein bestandener Transporttest kein Genauigkeitsnachweis.

Die am 29. Oktober dokumentierte Basis erreichte 173/173 gemeinsame gültige Fenster. Der RX1↔RX4-Paarlauf erreichte in beiden Richtungen 293/293 gültige Fenster; sein Nutzen für die Positionsschätzung bleibt offen. Eine 60-minütige TX-only-Aufnahme bestand nachträglich die Offline-Neubewertung; der ursprüngliche Online-Abschluss war fehlgeschlagen. Details und Grenzen: [Ergebnisse](../research/results.md).

Für Aufbau und Durchführung: [Installation](../setup/installation.md), [Firmware](../setup/firmware.md) und [historisches Messprotokoll](../project/03-messprotokoll.md). Das Messprotokoll enthält ältere versuchsspezifische Vorgaben; aktuelle Kriterien vor einer neuen Messung gesondert festlegen.

Quellen: [Projekt-README: Validierung](../../README.md#aktueller-validierungsstand) und [D4/D5/D6-Ergebnisbericht](../../experiment-reports/2026-08-23_D4-D5-D6_technischer-bericht/README.md).
