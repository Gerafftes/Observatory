# ESP32-C3 / HLK-LD2450 mmWave-Knoten

Die Radarknoten bilden einen von WiFi-CSI getrennten Referenzpfad. Radarziel-Slots sind keine dauerhaften Personenidentitäten. Zwei Sensoren sind für die gemeinsame Referenz vorgesehen; ihre Raum-, Zeit- und Genauigkeitsabnahme ist nicht durch eine erfolgreiche Installation erledigt.

## MMWAVE1 and MMWAVE2 upload

Der dokumentierte Quellweg verwendet ESP-IDF 5.4 und eigene Konfiguration/Buildausgabe für jeden stabilen Knoten `MMWAVE1` beziehungsweise `MMWAVE2`. Ein Bild darf nicht still auf den anderen Knoten kopiert werden. WLAN, Zielserver und OTA-Token sind private Konfiguration, kein Bestandteil dieses Dokumentationssatzes.

Die ursprüngliche USB-Erstinstallation mit `idf.py flash` schreibt Bootloader, Partitionstabelle, OTA-Initialdaten und App. Sie gehört zu einer ausdrücklich beauftragten Erstinstallation oder bewusst geplanten Vollinstallation, nicht zu einem pauschalen App-Update bestehender Geräte. Tatsächliches Layout, Sicherung und Rückweg sind vorher getrennt zu prüfen. Diese Darstellung führt keinen Upload oder Gerätestart aus.

Der historische MMWAVE2-Vergleich vom 26. September beschrieb eine funktionierende Assoziation bei 40 Viertel-dBm (10 dBm) und ein Scheitern bei 78. Das ist eine gerätegebundene damalige Beobachtung, keine allgemeine Einstellung für alle Boards. Die damals nötige private Konfiguration darf bei einem gezielt beauftragten Rebuild nicht verloren gehen.

## Koordinaten und Messrolle

Der Sensing-Server verwendet die gespeicherte Setuptransformation. Rohes `x_mm` ist die signierte laterale Sensorkoordinate, lokales `y_mm` zeigt vorwärts; Raum-X bezeichnet die Länge und Raum-Z die Breite. Yaw beschreibt Radar-vorwärts von Raum +X in Richtung +Z. Links-/Rechts-Spiegelung ist an bekannten Bodenmarken zu prüfen und nicht allein aus der UI oder der Platinenoberseite abzuleiten. Der dokumentierte Vertrag verlangt mindestens zwei bis drei vermessene Bodenmarken, bevor der Radar als Genauigkeitsreferenz dient.

Kalibrierungsradar darf synchronisierte WiFi-Trainingsdaten beschriften. Im Blindvergleich bleibt das WiFi-Modell zuvor eingefroren; Referenzdaten werden getrennt gespeichert und erst für die Bewertung zusammengeführt. Ein falscher Messmodus oder fehlende/alte Pakete ist kein gültiger Referenznachweis.

Weiter: [Firmware](../../../../docs/setup/firmware.md), [mmWave-Einordnung](../../../../docs/software/mmwave.md), [Breadboard](../../../../hardware/breadboard/README.md) und [historischer Transportvergleich](../../../../experiment-reports/2026-09-20_mmwave-sequenzverlustvergleich/README.md).
