# Quellen zu Wi-Fi-Sensing und Personenerfassung

34 Quellen zu CSI, Radar, Pose, Ortung und Vitalparametern. Stand: 5. Oktober 2026.

[Videos](#videos) · [Quellen nach Thema](#quellen-nach-thema) · [Hintergrundquellen](#hintergrundquellen) · [Prüfstand](#prüfstand)

## Videos

Kurzbeschreibungen aus den gelesenen YouTube-Transkripten.

| ID | Video | Kanal | Inhalt |
| --- | --- | --- | --- |
| V01 | [Capturing a Human Figure Through a Wall using RF Signals](https://www.youtube.com/watch?v=7LTr02cJkiA) | MIT CSAIL | RF Capture: Silhouetten durch Wände, Personenunterscheidung und Handverfolgung. |
| V02 | [Your WiFi Can See You. Here's How.](https://www.youtube.com/watch?v=0OdR8rRMz3I) | Bilawal Sidhu | Überblick über CSI, DensePose, WhoFi, Funkortung und Privatsphäre. |
| V03 | [Build a Wi-Fi Motion Detector](https://www.youtube.com/watch?v=7XwgxTXPJKM) | Innovate Yourself | ESP32 und LED mit **RSSI-Grenzwert**, obwohl der Titel CSI nennt. |
| V04 | [Measure Your Heart Rate with Wi-Fi!](https://www.youtube.com/watch?v=Cf6_PGuEiZY) | Nick Bild | Eigene Pulse-Fi-Nachimplementierung: ESP32-CSI, LSTM-Training und Vergleich mit Fingersensor. |
| V05 | [How to Stop Cops From Using Wi-Fi to See Through Walls](https://www.youtube.com/watch?v=LngDW3t36nc) | Hampton Law | Kommentar zu Überwachung, Routereinstellungen und US-Recht. |
| V06 | [AI Can See Without Cameras](https://www.youtube.com/watch?v=olaQ3-m271M) | Bilawal Sidhu | Überblick über Radar, kontaktlose Biometrie und umstrittene Ghost-Murmur-Berichte. |
| V07 | [This AI Senses Humans Through Walls](https://www.youtube.com/watch?v=kBFMsY5ZP0o) | Two Minute Papers | RF-Pose: kameragestütztes Training, mehrere Personen und Erfassung durch Wände. |

## Quellen nach Thema

### ESP32 und Wi-Fi

| ID | Quelle | Schwerpunkt |
| --- | --- | --- |
| G01 | [thejanu-dileepa / wifi-human-detector-esp32](https://github.com/thejanu-dileepa/wifi-human-detector-esp32) | Anwesenheit, CSI-Streaming und experimentelle Vitalparameter; PC-Klassifikator laut README mit RSSI-Fenstern. |
| G03 | [ruvnet / RuView](https://github.com/ruvnet/RuView) | Umfangreiches Projekt für Wi-Fi-Sensing, räumliche Erfassung und Vitalparameter. |
| G04 | [ashus3868 / Wi-Fi-Motion-Detector-ESP32](https://github.com/ashus3868/Wi-Fi-Motion-Detector-ESP32) | Begleitprojekt zu V03/T01; die dort gezeigte Umsetzung verwendet RSSI. |
| T01 | [ESP32-Bewegungsmelder — Tutorial](https://www.innovationyourself.com/wi-fi-motion-detector-with-csi/) | RSSI-Grenzwert und LED; Titel nennt CSI. |
| P10 | [From CSI to Coordinates (2025)](https://www.mdpi.com/1999-5903/17/9/395) | Vier ESP32 und AP; Random Forest für 43 Rasterpositionen. Beste Konfiguration: drei Empfänger, 82,12 % Klassifikationsgenauigkeit. Ein Raum; Daten auf Anfrage. |
| D03 | ESP-NOW v5.4.1: [GitHub-Quelle](https://github.com/espressif/esp-idf/blob/v5.4.1/docs/en/api-reference/network/esp_now.rst) · [API-Dokumentation](https://docs.espressif.com/projects/esp-idf/en/v5.4.1/esp32/api-reference/network/esp_now.html) | Offizielle Transportgrundlage: Frames, Kanäle und Callbacks. Sendebestätigung gilt auf MAC-Ebene; CSI-Erfassung benötigt eigene Dokumentation. |

### Körperpose und Wiedererkennung

| ID | Quelle | Schwerpunkt |
| --- | --- | --- |
| P01 | RF-Pose (2018): [CVPR-Paper](https://openaccess.thecvf.com/content_cvpr_2018/html/Zhao_Through-Wall_Human_Pose_CVPR_2018_paper.html) · [MIT](https://rfpose.csail.mit.edu/) · [Scribd](https://de.scribd.com/document/400896617/Zhao-Through-Wall-Human-Pose-CVPR-2018-Paper) | 2D-Pose aus Funksignalen, kameragestütztes Training, auch durch Wände. Scribd-Spiegel ungeprüft. |
| P02 | [DensePose From WiFi](https://arxiv.org/abs/2301.00250) | Wi-Fi-Signale auf die menschliche Körperoberfläche abbilden. |
| A01 | [RF Capture — MIT-Bericht (2015)](https://news.mit.edu/2015/wireless-x-ray-vision-could-power-virtual-reality-smart-homes-hollywood-1028) | Silhouetten, Personenunterscheidung und Bewegungsverfolgung; Bezug zu V01. |
| P03 | [WhoFi (2025)](https://arxiv.org/abs/2507.12869) | Personenwiedererkennung aus Wi-Fi-CSI mit neuronalen Modellen. |
| P08 | [RDGait (2024)](https://dl.acm.org/doi/10.1145/3678552) | Gangbasierte Personenwiedererkennung mit einem mmWave-Radarchip. |

### Vitalparameter

| ID | Quelle | Schwerpunkt |
| --- | --- | --- |
| G02 | [nickbild / csi_hr](https://github.com/nickbild/csi_hr) | Pulse-Fi-Nachimplementierung mit ESP32, CSI und LSTM; Bezug zu V04/P04. |
| P04 | [Pulse-Fi (2025)](https://ieeexplore.ieee.org/abstract/document/11096342) | Herzfrequenzschätzung aus Wi-Fi-CSI mit kostengünstiger Hardware. |
| P05 | [Through the wall human heart beat detection (2024)](https://pmc.ncbi.nlm.nih.gov/articles/PMC10847523/) | Herzschlagerfassung durch Wände mit einkanaligem 24-GHz-CW-Radar. |
| P09 | [Vital-Radio (CHI 2015)](https://people.csail.mit.edu/hongzi/content/publications/VitalRadio-CHI.pdf) | Kontaktlose Atem- und Herzfrequenzmessung, auch bei mehreren Personen und aus einem anderen Raum. |
| D02 | [Portable Signs of Life Identifier (2019)](https://www.dhs.gov/sites/default/files/publications/portable_signs_of_life_updated-508c_v3.pdf) | DHS-Übersicht zur Lebenszeichenerkennung für Einsatz- und Rettungskräfte. |
| W02 | [Google Nest Hub: Sleep Sensing](https://support.google.com/googlehome/answer/10357288) | Produktdokumentation zu radarbasierter Bewegungs-, Atem- und Schlafanalyse. |

### Radar, Datensätze und weitere Funktechnik

| ID | Quelle | Schwerpunkt |
| --- | --- | --- |
| P06 | [mmWave-Ortung in komplexen Innenräumen (2024)](https://www.mdpi.com/2072-4292/16/14/2572) | 77-GHz-FMCW-Radar; Unterdrückung von Störreflexionen und Mehrwege-Geisterzielen. |
| PA01 | [US20110025546A1 — Radar durch Wände](https://patents.google.com/patent/US20110025546A1/en) | Patentveröffentlichung für ein mobiles Sense-Through-the-Wall-Radarsystem. |
| P07 | [mmWave-Emotionsdatensatz (2026)](https://www.nature.com/articles/s41597-026-07159-6) | Radar, PPG, Hautleitfähigkeit und subjektive Bewertungen von 15 Teilnehmenden. |
| PA02 | [US11698453B2 — Mobilfunk-Umgebungserfassung](https://patents.google.com/patent/US11698453B2/en) | Erteiltes Patent zur Umgebungserfassung mit Mobilfunksignalen. |
| A04 | [Ghost Murmur — Scientific American](https://www.scientificamerican.com/article/what-is-the-quantum-ghost-murmur-purportedly-used-in-iran-scientists/) | Fachliche Kritik an behaupteter Herzschlagerkennung über große Entfernungen mittels Quantenmagnetometrie. |

## Hintergrundquellen

Diese acht Quellen liefern überwiegend Kontext oder Überblicke. Für Implementierung und Leistungsnachweise sind die jeweiligen Originalarbeiten, Code und Messdaten maßgeblich.

| ID | Quelle | Begrenzter technischer Nutzen |
| --- | --- | --- |
| A02 | [Soldiers see through walls](https://www.army.mil/article/32868/soldiers-see-through-walls/) | Radarvorführung ohne reproduzierbare Methode oder Messdaten. |
| A03 | [Jio: Ausbau von 26-GHz-5G](https://www.ril.com/news-media/press-releases/jio-announces-nationwide-rollout-5g-based-connectivity-using-26-ghz-mm) | Kommunikationsnetzausbau; belegt keine Sensing-Funktion. |
| A05 | [AP: Rettungsaktion im Iran](https://apnews.com/article/iran-war-fighter-jet-rescue-trump-7d8cfb6d0fd400abdc71f8c9d67408fe) | Nachrichtenbericht; konkrete Erfassungstechnik bleibt unbekannt. |
| W01 | [ZaiNar](https://zainartech.com/) | Leistungsversprechen; auf der geprüften Homepage keine offenen Algorithmen oder nachvollziehbaren Benchmarkdaten. |
| D01 | [SBIR: Biometrics-at-a-distance](https://www.sbir.gov/awards/142821) | Forschungsziele; ausreichende Methodendetails und Messdaten fehlen auf der Förderseite. |
| V02 | [Bilawal Sidhu: WiFi Can See You](https://www.youtube.com/watch?v=0OdR8rRMz3I) | Überblick und Wegweiser zu Originalquellen. |
| V06 | [Bilawal Sidhu: AI Can See Without Cameras](https://www.youtube.com/watch?v=olaQ3-m271M) | Überblick über Radar und Biometrie; Aussagen anhand der Primärbelege prüfen. |
| V05 | [Hampton Law: Wi-Fi und Polizei](https://www.youtube.com/watch?v=LngDW3t36nc) | US-Rechtskommentar; kaum Details zur Signalverarbeitung oder technischen Validierung. |

## Prüfstand

- **Videos:** Alle sieben Transkripte gelesen; V02–V06 automatisch generiert.
- **Lokale Volltexte:** P01 (10 Seiten), P06 (23), P07 (14), P09 (10), P10 (23). Titelseiten geprüft; bei P10 zusätzlich ausgewählte Methodik-, Ergebnis- und Einschränkungsabschnitte.
- **Offene Volltexte:** P04, P08 und D02. Bisher nur Veröffentlichungsangaben, Metadaten oder Auszüge geprüft.
- **Prüfbelege:** [UC Santa Cruz zu Pulse-Fi](https://news.ucsc.edu/2025/09/pulse-fi-wifi-heart-rate/), [Verlagsmetadaten zu P06](https://www.mdpi.com/2072-4292/16/14/2572/notes), [dblp zu RDGait](https://dblp.org/rec/journals/imwut/WangZWWFZ24.html).
- **Bereinigung:** Fünf doppelte Links zu P02, V03, V04, V06 und V07 entfernt; RF-Pose-Zugänge gebündelt und YouTube-Teilenparameter bereinigt.

Kurzbeschreibungen dienen zur Orientierung. Leistungsangaben sind Aussagen der Quellen und kein unabhängiger Nachweis für Observatory. Die Sammlung enthält öffentliche Links und eigene Beschreibungen; PDFs und Transkripte werden nicht mitveröffentlicht.
