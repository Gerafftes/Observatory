# Koordinatorreview: kanonische Speicherablage und Startreserve v3.1

**Die Speicheroptimierung besteht die unabhängige Offline-Gegenprüfung. Die geprüften Originalbelege bleiben vollständig erhalten. Der Hardwarelauf kann aktuell noch nicht starten: 66,051743 GiB berechnete Startreserve stehen bei der abschließenden Prüfung etwa 58,185 GiB frei gegenüber; es fehlen rund 7,87 GiB. Adapter und normale Hosts bleiben außerdem bis zur getrennten geprüften Liveübergabe gesperrt.**

Stand: 16. Oktober 2026, Europe/Berlin. Maßgeblich ist der [finale Abschluss v3.1](../../private/rx-speicheroptimierung-offline-abschluss-v31-20261015T224337Z/REPORT.private.md) mit BINDINGS, SOURCE-MANIFEST, CANDIDATES und Siegel. Seine unverändert referenzierte [Produktions-/Lastlieferung v3](../../private/rx-speicheroptimierung-offline-20261015-215434Z/REPORT.private.md) enthält die kanonischen neuen Originale. Der ältere Reproduktionshelfer ist überholt; die finale v3.1-Quelle zählt. Kein Agent gestartet/angeschrieben und keine Hardware, Installation, Hostaktivierung, Löschung oder Verschiebung durch diesen Review.

## Belegerhaltung und echte Einsparung

Der neue hohe vollständige 480-/960-s-Dateilauf umfasst **2.668.952.280 Byte** statt früher **4.541.353.902 Byte**. Fünf große zusätzliche Quelle-/Exportkopien mit zusammen **1.865.528.804 Byte** entfallen. Roh-IQ, ursprüngliche Paketbytes, PCAP, vollständiger Hosttrace einschließlich Empfangs-/Writerereignissen, Peerdatei und Clock-/Status-/Config-/Diagnose-/Recorderbelege bleiben erhalten.

Der Koordinator hat **alle sieben großen kanonischen Originale und ihre früheren Vergleichsdateien vollständig erneut gelesen**. SHA-256, Bytezahl und Zeilenzahl stimmen in jedem Paar exakt. Die neuen Originale sind reguläre Dateien ohne Hard-/Symlinkalias; ihre Inodes unterscheiden sich von den alten Vergleichsdateien. PCAP-Zeilenwerte sind lediglich Dateibyte-Statistik und keine Paketanzahl. Keine Quelle-/Exportkopie oder große neue Aufnahme durch den Review angelegt.

Die kleine v3.1-Ergänzung übernimmt rund **928 kB** Quellen/Verträge/Tests; keine vollständigen Buildcaches, SDK-Buildordner oder alten Lastverzeichnisse. Bestehende Caches und sämtliche Original-/Fehlversuchsdateien bleiben erhalten. Die vier Firmwarevarianten und die getrennten Host-/Dateiprogramme werden unverändert über Hashreferenzen weiterverwendet. Keine kompilierte Quelle geändert, **keine Neubauten erforderlich oder im Review ausgeführt**. Keine neue Kompression eingeführt.

## Startreserve unabhängig nachgerechnet

| Gleichzeitig vorgehaltener Bestandteil | GiB |
|---|---:|
| Originaldateigrenzen beider Phasen, ohne große Exportduplikate | **26,500** |
| Drei Hostlogs mit je 8 GiB plus zwei PCAP-stderr mit je 1 GiB | **26,000** |
| HTTP-/Clock-/Diagnose-/Start-/End-/Fehlerbelege | **1,000** |
| Normalisierung | **9,489** |
| Zwei vollständige Auswertungen | **3,038** |
| Vorbereitungs-/Allokations-/Endreserven | **0,025** |
| **Summe** | **66,051743** |

Exakt **70.922.518.528 Byte**, rund 13,95 GiB unter der früheren Pauschale. Die Rechnung wurde unabhängig aus Komponenten nachgebildet und stimmt mit dem ausführbaren Planner und dem vor den hohen Testdaten gebundenen Plan überein. Sie ist keine Garantie exklusiv reservierter freier Bytes und kein erwarteter Verbrauch. Der tatsächlich erzeugte synthetische Lauf mit 2,67 GB ist davon getrennt.

Der größte zusätzliche konservative Posten sind **26 GiB Prozesslogs**. Deren neue Größenbegrenzungen sind tatsächlich im NativeOS-Pfad durchgesetzt und mit abgefangenem Prozessdispatch geprüft. Aus dem bloßen Logetat folgt keine technisch unvermeidbare Nutzdatenmenge. Eine weiter kleinere Loggrenze müsste gesondert anhand erlaubter Logausgaben, Fehlerbelege, Integrität und Endverhalten qualifiziert werden; sie wurde hier nicht ohne Nachweis geändert. Die heute geprüfte Startschranke bleibt 66,05 GiB.

Ein gemeinsames gebundenes Dateisystem mit 4.096-Byte-Allokation ist Voraussetzung. Abweichende Geräte-/Mount-/Blockbindung oder unzureichender Platz sperren den nächsten Schritt; keine doppelte Zusage derselben APFS-Kapazität. Vorhandene Präparationsdateien zählen nicht als nachträglich neu gewonnener Platz. Ursprünglicher Host-CWD bleibt getrennt gebundene Beobachtungs-/Beendigungsidentität; ausschließlich neue Writer-CWDs sind neue Schreibziele. Diese in v3.1 korrigierten Übergaben sind erneut geprüft.

## Unabhängig ausgeführte Prüfung

- Finale schreibgeschützte Verifikation **1.351 Manifestdateien, 56 Quellbindungen**; ursprüngliche Produktions-/Lastlieferung **3.967 Manifestdateien, 58 Quellbindungen und 154 Read-only-Eingänge** erneut bestanden. Kein Cache-/Compilerneubau oder normale Hostausführung.
- Echte Kern-/Adapter-/Recorder-/Vorklammerpfade mit gesperrten nativen Datei-/Netz-/Geräte-/Prozesseinstiegen erneut ausgeführt: **34 + 26 + 10 + 8 Prüfungen**, einschließlich **292 Recorder-Szenariodurchläufen**, bestanden.
- Alle **16 Speicher-/Integritäts-/Ausgabe-/Native-FD-Prüfgruppen**, **vier Initial-CWD-Gegenfälle** und **27 tatsächlichen Normalisierungsgegenfälle** erneut bestanden. Exakt ausreichende Bytes, ein Byte zu wenig, Mount-/Pfadwechsel, veränderte Originale, falsche Rollen, fehlende Belege und Writerfehler bleiben echte Abbruch-/Nichtbeurteilbarkeitsfälle.
- Vollständigen unveränderten hohen neuen Dateibestand mit den **finalen v3.1-Lesern erneut normalisiert und ausgewertet**. Fachliche Runberichte stimmen exakt; **zwölf Fenstertabellen bytegleich**. Gesamt-JSON enthält erwartungsgemäß neue Normalisierungs-/Herkunftspfade und ist deshalb nicht als vollständiges Bytegleichheitsobjekt ausgegeben. Die bereits gespeicherte deterministische Auswertungswiederholung derselben Eingänge bleibt zusätzlich versiegelt.
- Hoher ursprünglicher Produzentenlauf: Basis 480 s mit 50/50/50/50 CSI/s; Paar 960 s mit TX1 40/50/50/40 und je zehn Peerframes/s, plus Sync. Qualität **473/473 beziehungsweise 953/953 gemeinsame Fenster**, je Richtung **9.600 Peerpakete**. Diese Produktion wurde im Review nicht nochmals erzeugt. Der vollständige Daten-/Konsumentenpfad wurde erneut gelesen; geerbte vollständige SDK-/Rust-/Raten-/Radar-Suiten werden nicht als hier frisch ausgeführt ausgegeben.

Das hohe Testmodell setzt ausreichend freie Livebytes synthetisch an; dies ist keine Feststellung realer Deckung. Tatsächliche Firmwarefrische, Ressourcen und Positionsleistung bleiben unbekannt; Gesamturteil weiterhin korrekt NICHT_BEURTEILBAR. Software-PASS, reale Speicherbeobachtung und spätere Funkabnahme bleiben getrennt.

Alle ausgeführten Produktions-/Testkinder liefen unter Prozess-Netzverbot und Schreibrechten nur in der neuen Reviewablage. Beim gelieferten Reproduzierer wurde ausschließlich seine Sandbox auf diese engere Regel begrenzt; die fachlichen Quellen blieben unverändert. Der öffentliche Arbeitsstatus war vor den Koordinator-Dokumentationsänderungen unverändert. [Private Reviewreceipts](../../private/rx-speicheroptimierung-koordinatorreview-20261016/REVIEW.private.json).

## Hardwareentscheidung und nächster Schritt

**Aktuell nicht starten.** Bei der letzten unabhängigen Messung: **58,185 GiB frei**, Bedarf **66,052 GiB**, Lücke **7,867 GiB**. Zahlen sind Momentaufnahmen; unmittelbar vor einer späteren Freigabe erneut prüfen. Der Agentenabschluss hatte noch etwa 60,60 GiB gemessen; dieser ältere Wert ist kein aktueller Freiplatznachweis.

Nächster konkreter Schritt: **die Deckung der geprüften Startreserve herstellen**, ohne eigenmächtige Löschung oder erneute Firmwarearbeit. Ob durch ein anderes erlaubtes Speicherziel, ausdrücklich autorisierte Platzfreigabe oder eine separat qualifizierte engere Logplanung, ist eine eigene Entscheidung. Keine dieser Maßnahmen wurde hier ausgeführt; die 80-GiB-Pauschale darf nicht wieder eingesetzt und die geprüfte 66-GiB-Grenze nicht bloß passend verkleinert werden.

Danach den bereits vorbereiteten begrenzten Hardwareauftrag mit der geprüften v3.1-Datei-/Speicherbindung, eigener ausführbarer Livekopie/-hostbinarys, frischen Geräte-/Host-/Geometriebindungen und getrennten passenden Freigaben konkretisieren. Die unveränderten Offlineoriginale bleiben gesperrt. Höchstens 82 normale/maximal 120 GETs einschließlich zwölf End-GETs, zwei App-POSTs nur RX1/RX4, 480/960 s, vier Recordersteuerungen, drei Hostwechsel und 6.000 s einschließlich 600 s Endreserve bleiben erhalten.

Hardware: TX1 und RX1–RX4 normal an festen Positionen versorgen, Mac im bestehenden Projekt-WLAN und am Netzteil. Keine COM-/Mac-USB-Datenverbindung zu den ESPs, keine Radare für diesen reinen Funkvergleich erforderlich. Ein anderes Speicherziel ist nicht automatisch eine Aufforderung, neue Hardware zu kaufen.

RX2/RX3 bleiben im zukünftigen Paarauftrag unverändert. Firmware-/Peerwirkung im Basis→Paarvergleich bleibt kombiniert. Die drei vollständigen Livekriterien des 14. Oktober bleiben offen; **weiterhin 2/5**, kein zusätzliches Häkchen aus diesem Speicherabschluss. **Schwierigkeit des anschließenden Hardwaregesamtlaufs: 4/5**; gezielte Speicherklärung 3/5. Kein Agent gestartet und kein Hardwarebeginn durch diesen Review.
