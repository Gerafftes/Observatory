# Quellensammlung zu Wi-Fi-Sensing und funkbasierter Personenerfassung

Stand der Sammlung: 5. Oktober 2026.

Diese Sammlung umfasst die bereitgestellten Videos, wissenschaftlichen Veröffentlichungen, GitHub-Projekte, Patente und Kontextquellen zu Wi-Fi-Sensing, Radar, Personenerfassung und kontaktloser Biometrie. Die Kurzbeschreibungen dienen zur Orientierung. Sie sind keine unabhängige Bestätigung der technischen, kommerziellen oder rechtlichen Aussagen einer Quelle und kein Nachweis für die Leistungsfähigkeit von Observatory.

Die ursprünglichen 39 Links wurden auf 34 eindeutige URLs bereinigt. Drei Zugänge zur RF-Pose-Veröffentlichung sind unter P01 gebündelt. Die ursprüngliche Liste umfasst damit 32 Sammeleinträge. Am 5. Oktober 2026 wurden das Ortungspaper P10 und die ESP-NOW-Dokumentation D03 ergänzt. Die Sammlung enthält jetzt **34 Sammeleinträge: sieben Videos und 27 weitere Ressourcen**. Die IDs bleiben für spätere Ergänzungen und Verweise stabil. Bereitgestellte PDFs bereits erfasster Veröffentlichungen zählen als zusätzliche Volltextzugänge, nicht als neue Quellen.

## Inhalt

- [Videos](#videos)
- [Wi-Fi: Anwesenheit und Bewegung](#wi-fi-anwesenheit-und-bewegung)
- [Wi-Fi: Ortung mit ESP32-CSI](#wi-fi-ortung-mit-esp32-csi)
- [ESP32: Datenerfassung und Transport](#esp32-datenerfassung-und-transport)
- [Körperpose und Silhouetten aus Funksignalen](#körperpose-und-silhouetten-aus-funksignalen)
- [Personenidentität und Wiedererkennung](#personenidentität-und-wiedererkennung)
- [Vitalparameter: Herzschlag, Atmung und Schlaf](#vitalparameter-herzschlag-atmung-und-schlaf)
- [Radar: Personenerkennung und Ortung](#radar-personenerkennung-und-ortung)
- [Emotionserkennung und Forschungsdatensätze](#emotionserkennung-und-forschungsdatensätze)
- [Funknetze: Ortung, Umgebungserfassung und Infrastruktur](#funknetze-ortung-umgebungserfassung-und-infrastruktur)
- [Ghost Murmur: Berichterstattung und technische Einordnung](#ghost-murmur-berichterstattung-und-technische-einordnung)
- [Quellen mit begrenztem direktem technischem Nutzen](#quellen-mit-begrenztem-direktem-technischem-nutzen)
- [Bereinigung und Prüfstand](#bereinigung-und-prüfstand)

## Videos

Die Kurzbeschreibungen beruhen auf abgerufenen und gelesenen YouTube-Transkripten. V01 und V07 besitzen englische Untertitel ohne Kennzeichnung als automatisch generiert. V02 bis V05 besitzen automatisch generierte englische Transkripte; das gelesene Transkript von V06 ist automatisch generiert und deutsch. Automatische Transkripte enthalten teilweise Fehler bei Eigennamen. Die Transkripte selbst sind nicht Bestandteil dieser Datei.

| ID | Video | Kanal | Kurze Beschreibung | Bezug zur Sammlung |
| --- | --- | --- | --- | --- |
| V01 | [Capturing a Human Figure Through a Wall using RF Signals](https://www.youtube.com/watch?v=7LTr02cJkiA) | MIT CSAIL | Demonstriert RF Capture: Ein Funkgerät erfasst Reflexionen eines Menschen hinter einer Wand und kombiniert mehrere Momentaufnahmen zu einer Silhouette. Gezeigt werden außerdem die Unterscheidung von Personen und Körperhaltungen sowie das Verfolgen einer Handbewegung im Vergleich zu Kinect. | A01 |
| V02 | [Your WiFi Can See You. Here's How.](https://www.youtube.com/watch?v=0OdR8rRMz3I) | Bilawal Sidhu | Erklärt Wi-Fi-Sensing anhand von CSI, Xfinity WiFi Motion, DensePose und WhoFi. Anschließend behandelt das Video ZaiNar-Funkortung, Andurils elektromagnetische Aufklärung und die satellitengestützte Ortung von Funksendern. Diskutiert Hausautomation, KI und Folgen für die Privatsphäre. | P02, P03, W01 |
| V03 | [Build a Wi-Fi Motion Detector: ESP32 & CSI Practical Guide](https://www.youtube.com/watch?v=7XwgxTXPJKM) | Innovate Yourself | Anleitung für einen ESP32 mit LED und ESP-IDF. Der Autor verändert das Beispiel `fast_scan`, liest die RSSI-Signalstärke aus und schaltet die LED beim Unterschreiten eines Grenzwerts. Die gezeigte Umsetzung verwendet RSSI, obwohl der Titel CSI nennt. | G04, T01 |
| V04 | [Measure Your Heart Rate with Wi-Fi! (DIY Project)](https://www.youtube.com/watch?v=Cf6_PGuEiZY) | Nick Bild | Stellt eine eigene Pulse-Fi-Nachimplementierung vor: Zwei ESP32 liefern CSI-Daten, eine Verarbeitungskette bereitet diese auf und ein LSTM schätzt die Herzfrequenz. Der Autor erläutert das Training mit Referenzmessungen und zeigt einen Livevergleich mit einem Fingersensor. | G02, P04 |
| V05 | [How to Stop Cops From Using Wi-Fi to “See Through the Walls” of Your Home](https://www.youtube.com/watch?v=LngDW3t36nc) | Hampton Law | Jeff Hampton diskutiert Wi-Fi-Sensing als mögliches Überwachungsinstrument, den Zugang zu Unternehmensdaten und den Schutz der Wohnung im US-Recht. Er zieht Vergleiche zu den Fällen Carpenter und Kyllo. Abschließend empfiehlt er, Bewegungs- und Anwesenheitserkennung sowie bestimmte Datenfreigaben in Routereinstellungen zu deaktivieren. | Datenschutz und US-rechtliche Einordnung |
| V06 | [AI Can See Without Cameras. WiFi Was Just the Beginning.](https://www.youtube.com/watch?v=olaQ3-m271M) | Bilawal Sidhu | Überblick über Radar, mmWave und kontaktlose Biometrie: Sense-Through-the-Wall, Herzschlagerfassung, Ortung, Emotionsforschung und Gangwiedererkennung. Behandelt Google Soli/Nest Hub, 26-GHz-5G, ein T-Mobile-Patent und günstige Sensormodule. Abschließend diskutiert der Autor die umstrittenen Berichte über Ghost Murmur. | P05–P09, W02, A02–A05, D01, D02, PA01, PA02 |
| V07 | [This AI Senses Humans Through Walls 👀](https://www.youtube.com/watch?v=kBFMsY5ZP0o) | Two Minute Papers | Erklärt RF-Pose und das Lehrer-Schüler-Prinzip: Ein kamerabasiertes Modell liefert beim Training Körperposen, während ein zweites Netz lernt, diese aus Funksignalen zu schätzen. Gezeigt wird die Erfassung mehrerer Personen, bei schlechter Beleuchtung und durch Wände. | P01 |

## Wi-Fi: Anwesenheit und Bewegung

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| G01 | [thejanu-dileepa / wifi-human-detector-esp32](https://github.com/thejanu-dileepa/wifi-human-detector-esp32) | GitHub | ESP32-S3-Projekt für Anwesenheit und Bewegung, mit CSI-Streaming und experimenteller Vitalparameter-Auswertung. Laut README verwendet der PC-seitige Aktivitätsklassifikator RSSI-Fenster. |
| G03 | [ruvnet / RuView](https://github.com/ruvnet/RuView) | GitHub | Umfangreiches Projekt für Wi-Fi-Sensing, räumliche Erfassung, Anwesenheit und Vitalparameter. Berührt mehrere Themen dieser Sammlung. |
| G04 | [ashus3868 / Wi-Fi-Motion-Detector-ESP32](https://github.com/ashus3868/Wi-Fi-Motion-Detector-ESP32) | GitHub | ESP32-Code zum Bewegungsmelder-Tutorial T01 und Video V03. Die dort gezeigte Umsetzung wertet RSSI-Signalstärke aus. |
| T01 | [Build a Wi-Fi Motion Detector: ESP32 & CSI Practical Guide](https://www.innovationyourself.com/wi-fi-motion-detector-with-csi/) | Tutorial / Blog | Anleitung mit ESP-IDF, Routerverbindung und LED-Ausgabe. Der Beispielcode verwendet einen RSSI-Grenzwert, obwohl der Titel CSI nennt. |

## Wi-Fi: Ortung mit ESP32-CSI

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| P10 | [From CSI to Coordinates: An IoT-Driven Testbed for Individual Indoor Localization](https://www.mdpi.com/1999-5903/17/9/395) | Journalpaper, Future Internet 2025, 17, 395; DOI: 10.3390/fi17090395 | Originalarbeit mit vier ESP32-WROOM-32E, einem Access Point und UDP-Datenerfassung. Beschreibt CSI-Amplitudenaufbereitung, statistische Merkmale und Random-Forest-Klassifikation von 43 Rasterpositionen. Die beste Konfiguration mit drei Empfängern erreicht laut Paper 82,12 % Klassifikationsgenauigkeit; dies ist keine Angabe zur metrischen Positionsgenauigkeit. Untersuchung in einem Raum; Daten nur auf Anfrage erhältlich. Lokaler Volltext bereitgestellt. |

## ESP32: Datenerfassung und Transport

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| D03 | **Espressif ESP-NOW, ESP-IDF v5.4.1** · [Dokumentationsquelle auf GitHub](https://github.com/espressif/esp-idf/blob/v5.4.1/docs/en/api-reference/network/esp_now.rst) · [Gerenderte Dokumentation für ESP32](https://docs.espressif.com/projects/esp-idf/en/v5.4.1/esp32/api-reference/network/esp_now.html) | Offizielle API- und Protokolldokumentation | Direkte Implementierungsgrundlage für verbindungslose WLAN-Kommunikation: Frameformat, Peer- und Kanalregeln, Send-/Empfangs-Callbacks und Datenraten. Erläutert, dass ein erfolgreicher Sendecallback den Empfang auf der MAC-Ebene bestätigt und bei Bedarf Anwendungs-ACKs und Sequenznummern erforderlich sind. Beschreibt den Pakettransport; die CSI-Erfassung und ihre Auswertung benötigen eigene Quellen. |

## Körperpose und Silhouetten aus Funksignalen

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| P01 | **Through-Wall Human Pose Estimation Using Radio Signals — RF-Pose** · [CVPR-Paper](https://openaccess.thecvf.com/content_cvpr_2018/html/Zhao_Through-Wall_Human_Pose_CVPR_2018_paper.html) · [MIT-Projektseite](https://rfpose.csail.mit.edu/) · [Scribd-Zugang](https://de.scribd.com/document/400896617/Zhao-Through-Wall-Human-Pose-CVPR-2018-Paper) | Paper + Projektseite, 2018 | Schätzung menschlicher 2D-Körperposen aus Funksignalen, auch durch Wände und bei Verdeckung. Lokaler Originalvolltext bereitgestellt. Drei Onlinezugänge gebündelt; der Scribd-Spiegel ist anhand des Linktitels zugeordnet, sein Volltext wurde nicht verifiziert. Bezug zu V07. |
| P02 | [DensePose From WiFi](https://arxiv.org/abs/2301.00250) | Preprint, arXiv:2301.00250 | Übertragung von Wi-Fi-Signalen auf dichte Zuordnungen zur menschlichen Körperoberfläche. Bezug zu V02. |
| A01 | [How wireless “X-ray vision” could power virtual reality, smart homes, and Hollywood](https://news.mit.edu/2015/wireless-x-ray-vision-could-power-virtual-reality-smart-homes-hollywood-1028) | MIT-Forschungsbericht, 2015 | Vorstellung von RF Capture: Rekonstruktion menschlicher Silhouetten aus Funkreflexionen sowie Unterscheidung von Personen und Bewegungen. Bezug zu V01. |

## Personenidentität und Wiedererkennung

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| P03 | [WhoFi: Deep Person Re-Identification via Wi-Fi Channel Signal Encoding](https://arxiv.org/abs/2507.12869) | Preprint, 2025 | Wiedererkennung von Personen anhand biometrischer Merkmale in Wi-Fi-CSI und eines neuronalen Modells. |
| P08 | [RDGait: A mmWave Based Gait User Recognition System for Complex Indoor Environments Using Single-chip Radar](https://dl.acm.org/doi/10.1145/3678552) | Journalpaper, 2024 | Wiedererkennung von Personen anhand ihres Gangs mit einem mmWave-Radarchip in komplexen Innenräumen. Titel bibliografisch bestätigt; ACM-Volltext nicht abrufbar. |

## Vitalparameter: Herzschlag, Atmung und Schlaf

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| G02 | [nickbild / csi_hr](https://github.com/nickbild/csi_hr) | GitHub | Eigene Nachimplementierung des Pulse-Fi-Ansatzes zur Herzfrequenzschätzung mit ESP32, CSI-Verarbeitung und LSTM. Bezug zu P04 und V04. |
| P04 | [Pulse-Fi: A Low-Cost System for Accurate Heart Rate Monitoring Using Wi-Fi Channel State Information](https://ieeexplore.ieee.org/abstract/document/11096342) | Konferenzpaper, 2025 | Herzfrequenzschätzung aus Wi-Fi-CSI mit kostengünstiger Hardware und maschinellem Lernen. IEEE-Volltext nicht abrufbar; Zuordnung über den Bericht der UC Santa Cruz bestätigt (siehe Prüfstand). |
| P05 | [Through the wall human heart beat detection using single channel CW radar](https://pmc.ncbi.nlm.nih.gov/articles/PMC10847523/) | Journalpaper, 2024 | Untersuchung der Herzschlagerfassung durch Wände mit einkanaligem 24-GHz-CW-Radar. |
| P09 | [Smart Homes that Monitor Breathing and Heart Rate — Vital-Radio](https://people.csail.mit.edu/hongzi/content/publications/VitalRadio-CHI.pdf) | Konferenzpaper / PDF, CHI 2015 | Kontaktlose Erfassung von Atmung und Herzfrequenz mit Funksignalen; untersucht auch mehrere Personen und Messungen aus einem anderen Raum. |
| D01 | [Biometrics-at-a-distance](https://www.sbir.gov/awards/142821) | Forschungsförderung, SBIR / DARPA, 2013 | Radar-Forschungsprojekt zur Ortung von Personen hinter Hindernissen und Erfassung ihrer Atmung, Herzfrequenz und Körperbewegungen. |
| D02 | [Portable Signs of Life Identifier](https://www.dhs.gov/sites/default/files/publications/portable_signs_of_life_updated-508c_v3.pdf) | DHS-Technologiebericht / PDF, 2019 | Übersicht über Technologien und Geräte zur Erkennung von Lebenszeichen, insbesondere für Einsatz- und Rettungskräfte. Dokumenttitel und Auszüge bestätigt; vollständiger PDF-Abruf blockiert. |
| W02 | [Learn about Sleep Sensing on Google Nest Hub (2nd gen)](https://support.google.com/googlehome/answer/10357288) | Produktdokumentation | Radarbasierte Bewegungs- und Atemerfassung für die Schlafanalyse, ergänzt durch weitere Gerätesensoren. |

## Radar: Personenerkennung und Ortung

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| P06 | [Millimeter-Wave Radar Detection and Localization of a Human in Indoor Complex Environments](https://www.mdpi.com/2072-4292/16/14/2572) | Journalpaper, 2024 | Erkennung und Lokalisierung einer Person mit 77-GHz-FMCW-Radar in komplexen Innenräumen. Beschreibt die Unterdrückung von Störreflexionen und Mehrwege-Geisterzielen. Lokaler Originalvolltext bereitgestellt; die frühere Abruflücke ist damit geschlossen. |
| PA01 | [US20110025546A1 — Mobile sense through the wall radar system](https://patents.google.com/patent/US20110025546A1/en) | Patentveröffentlichung | Mobiles Radarsystem zur Erfassung von Zielen hinter Wänden. |
| A02 | [Soldiers see through walls](https://www.army.mil/article/32868/soldiers-see-through-walls/) | US-Army-Artikel, 2010 | Vorstellung eines Sense-Through-the-Wall-Radars, das Entfernung und grobe Richtung von Zielen anzeigt. |

## Emotionserkennung und Forschungsdatensätze

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| P07 | [An emotion recognition dataset using millimeter wave radar and physiological reference signals](https://www.nature.com/articles/s41597-026-07159-6) | Datensatzpaper, 2026 | Datensatz mit mmWave-Radar, PPG, Hautleitfähigkeit und subjektiven Emotionsbewertungen. Relevant für Vitalparameter-Auswertung und multimodale Emotionsforschung. |

## Funknetze: Ortung, Umgebungserfassung und Infrastruktur

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| W01 | [ZaiNar](https://zainartech.com/) | Unternehmensseite | Herstellerdarstellung kommerzieller Funkortung mithilfe präziser Netzwerksynchronisation; behandelt Wi-Fi, 5G und weitere Funkprotokolle. |
| PA02 | [US11698453B2 — Environment scanning using a cellular network](https://patents.google.com/patent/US11698453B2/en) | Erteiltes Patent | Umgebungserfassung mithilfe der Signale eines Mobilfunknetzes. |
| A03 | [Jio Announces Nationwide Rollout of 5G-Based Connectivity Using 26 GHz mm-Wave Spectrum](https://www.ril.com/news-media/press-releases/jio-announces-nationwide-rollout-5g-based-connectivity-using-26-ghz-mm) | Pressemitteilung, 2023 | Kontext zum Ausbau von 26-GHz-mmWave für 5G-Kommunikation. Die Quelle behandelt den Netzausbau; sie ist kein Nachweis für eine konkrete Personenerfassungsfunktion dieses Netzes. |

## Ghost Murmur: Berichterstattung und technische Einordnung

| ID | Ressource | Typ | Inhalt / Schwerpunkt |
| --- | --- | --- | --- |
| A04 | [Is the ‘Ghost Murmur’ quantum device possible? Scientists are skeptical](https://www.scientificamerican.com/article/what-is-the-quantum-ghost-murmur-purportedly-used-in-iran-scientists/) | Wissenschaftsjournalismus, 2026 | Kritische Einordnung der behaupteten Herzschlagerkennung über große Entfernungen mittels Quantenmagnetometrie. |
| A05 | [Trump details rescue of US crew downed in Iran](https://apnews.com/article/iran-war-fighter-jet-rescue-trump-7d8cfb6d0fd400abdc71f8c9d67408fe) | Nachrichtenartikel, 2026 | Kontextbericht zur Rettungsaktion und zu öffentlichen Aussagen über eingesetzte Technologien. Der Bericht ist kein technischer Nachweis für Ghost Murmur. |

## Quellen mit begrenztem direktem technischem Nutzen

Diese acht Einträge dienen vor allem als Hintergrund, Überblick oder Ausgangspunkt für weitere Recherche. Die Einordnung bezieht sich auf ihren Nutzen für die technische Umsetzung und Validierung von Observatory. Sie bleiben in der Sammlung erhalten; die Tabelle ergänzt ihre thematische Zuordnung. Für konkrete Methoden und Leistungsnachweise sind die jeweils zugrunde liegenden Originalarbeiten, Code und Messdaten heranzuziehen. Die Bewertung der Videos beruht auf ihren gelesenen Transkripten, die der übrigen Quellen auf den zugänglichen Seiteninhalten.

| ID | Quelle | Einordnung | Begründung / verbleibender Nutzen |
| --- | --- | --- | --- |
| A02 | [Soldiers see through walls](https://www.army.mil/article/32868/soldiers-see-through-walls/) | Historischer Demonstrationsbericht | Beschreibt eine Radarvorführung und allgemeine Funktionen. Enthält keine reproduzierbare Methode, Messdaten oder detaillierte Auswertung; geeignet als historischer Anwendungskontext. |
| A03 | [Jio: Ausbau von 26-GHz-5G](https://www.ril.com/news-media/press-releases/jio-announces-nationwide-rollout-5g-based-connectivity-using-26-ghz-mm) | Pressemitteilung zum Netzausbau | Belegt die Ankündigung von Kommunikationsinfrastruktur. Die Verwendung von 26 GHz allein belegt keine Personen- oder Herzschlagerfassung und liefert keine Sensing-Implementierung. |
| A05 | [AP-Bericht zur Rettungsaktion im Iran](https://apnews.com/article/iran-war-fighter-jet-rescue-trump-7d8cfb6d0fd400abdc71f8c9d67408fe) | Nachrichtenbericht | Gibt öffentliche Aussagen über eingesetzte Technologien wieder. Die konkrete Erfassungstechnik bleibt unbekannt; daraus lässt sich keine technische Umsetzung ableiten. |
| W01 | [ZaiNar-Unternehmensseite](https://zainartech.com/) | Herstellerdarstellung | Beschreibt Produkte und Leistungsversprechen. Auf der geprüften Homepage fehlen offengelegte Algorithmen, Rohdaten und ein nachvollziehbares Prüfverfahren für diese Aussagen; geeignet als Markt- und Produktkontext. |
| D01 | [SBIR: Biometrics-at-a-distance](https://www.sbir.gov/awards/142821) | Förderprojektbeschreibung | Belegt ein gefördertes Forschungsvorhaben und dessen Ziele. Enthält keine ausreichenden Methodendetails oder Messdaten, um die genannten Fähigkeiten nachzuvollziehen. |
| V02 | [Your WiFi Can See You. Here's How. — Bilawal Sidhu](https://www.youtube.com/watch?v=0OdR8rRMz3I) | Sekundärer Überblick | Fasst Wi-Fi-Sensing und weitere Ortungstechnologien zusammen. Nützlich als Wegweiser zu P02, P03 und anderen Originalquellen; liefert keine eigene reproduzierbare Methode oder Messreihe. |
| V06 | [AI Can See Without Cameras. WiFi Was Just the Beginning. — Bilawal Sidhu](https://www.youtube.com/watch?v=olaQ3-m271M) | Sekundärer Überblick | Fasst Radar, kontaktlose Biometrie und weitere Technologien zusammen. Nützlich zum Auffinden von Originalquellen; technische Aussagen und Übertragungen zwischen den Verfahren benötigen die jeweiligen Primärbelege. |
| V05 | [How to Stop Cops From Using Wi-Fi to See Through the Walls of Your Home — Hampton Law](https://www.youtube.com/watch?v=LngDW3t36nc) | Überwachungs- und Rechtskommentar | Schwerpunkt auf Überwachung und US-Recht. Liefert für unsere Signalverarbeitung, Hardware oder technische Validierung kaum verwertbare Details. |

## Bereinigung und Prüfstand

### Entfernte Dopplungen

Jeweils die zweite Nennung dieser fünf Quellen wurde entfernt:

- P02: `arxiv.org/abs/2301.00250`
- V03: YouTube-ID `7XwgxTXPJKM`
- V04: YouTube-ID `Cf6_PGuEiZY`
- V06: YouTube-ID `olaQ3-m271M`
- V07: YouTube-ID `kBFMsY5ZP0o`

YouTube-Kurzlinks wurden in einheitliche `https://www.youtube.com/watch?v=…`-Links umgewandelt. Die Teilen-Parameter `si` wurden entfernt. Paper, Projektseiten, Repositories, Tutorials und Videos zum selben Thema bleiben als unterschiedliche Ressourcen erhalten. Unter P01 sind die CVPR-Seite, die MIT-Projektseite und der anhand seines Linktitels zugeordnete Scribd-Zugang gebündelt.

### Grundlagen der Beschreibungen

- Für alle sieben Videos wurden YouTube-Transkripte abgerufen und gelesen. Ein Drittanbieter war dafür nicht erforderlich.
- Die übrigen Beschreibungen beruhen auf den zuvor recherchierten Projektseiten, Veröffentlichungsinformationen, Abstracts und verfügbaren Dokumentinhalten. Es wurde keine vollständige wissenschaftliche oder technische Bewertung aller Quellen durchgeführt.
- Bei P01 konnte die CVPR-Seite zunächst nicht direkt abgerufen werden. Am 5. Oktober 2026 wurde das Originalpaper als lokales PDF bereitgestellt und anhand seiner Titelseite bestätigt. Der Scribd-Volltext wurde nicht geprüft.
- Bei P04 war der IEEE-Volltext nicht abrufbar. Der ergänzend zur Identifikation verwendete [Bericht der UC Santa Cruz zu Pulse-Fi](https://news.ucsc.edu/2025/09/pulse-fi-wifi-heart-rate/) bestätigt die Zuordnung. Dieser Prüfbeleg zählt nicht als zusätzlicher Eintrag der ursprünglichen Sammlung.
- Bei P06 wurde der Titel zunächst über die [Versionshinweise des Verlags](https://www.mdpi.com/2072-4292/16/14/2572/notes) bestätigt. Der am 5. Oktober 2026 bereitgestellte lokale Originalvolltext schließt die frühere Abruflücke. Bei P08 wurde der Titel anhand [bibliografischer Metadaten in dblp](https://dblp.org/rec/journals/imwut/WangZWWFZ24.html) bestätigt; der ACM-Volltext war nicht abrufbar. Diese Prüfbelege zählen nicht als zusätzliche Sammeleinträge.
- Bei D02 waren Titel und indexierte Auszüge verfügbar; der vollständige PDF-Abruf war blockiert.
- Bei P10 wurden Titel, Abstract, Versuchsaufbau, Klassifikationsverfahren, Ergebnisauszüge, Einschränkungen und Datenverfügbarkeit im bereitgestellten PDF geprüft. Dies ist keine vollständige unabhängige Prüfung der Studie oder ihrer Ergebnisse.
- D03 wurde anhand der Dokumentationsquelle und der gerenderten offiziellen Dokumentation für ESP-IDF v5.4.1 geprüft.

### Bereitgestellte Volltexte vom 5. Oktober 2026

Die Identität der fünf PDFs wurde anhand von Titelseiten und Veröffentlichungsangaben geprüft. Die Dateien verbleiben außerhalb dieser GitHub-Dokumentationsablage; hier werden ausschließlich die öffentlichen Veröffentlichungslinks geführt.

| ID | Bereitgestellte Datei | Seiten | Einordnung |
| --- | --- | --- | --- |
| P07 | `s41597-026-07159-6.pdf` | 14 | Volltext des bereits erfassten Datensatzpapers. |
| P10 | `futureinternet-17-00395.pdf` | 23 | Neue Originalarbeit zur Ortung mit ESP32-CSI. |
| P06 | `remotesensing-16-02572-v2.pdf` | 23 | Volltext schließt die frühere Abruflücke. |
| P09 | `VitalRadio-CHI.pdf` | 10 | Lokaler Volltext des bereits erfassten Vital-Radio-Papers. |
| P01 | `Zhao_Through-Wall_Human_Pose_CVPR_2018_paper.pdf` | 10 | Lokaler Originalvolltext des RF-Pose-Papers. |

Weiterhin fehlen hier die Volltexte von P04 (Pulse-Fi), P08 (RDGait) und D02 (Portable Signs of Life Identifier). Fehlender Volltextzugriff ist kein Urteil über den technischen Nutzen einer Quelle.

Die Datei enthält ausschließlich öffentliche Quellenlinks und eigene Kurzbeschreibungen. Lokale Transkriptpfade, vollständige Transkripte, private Messdaten und Geräteinformationen sind nicht enthalten.
