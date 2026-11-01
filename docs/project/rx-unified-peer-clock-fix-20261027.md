# Peer-Zeitordnung: Offline-Reproduktion und Kandidat

Der vorzeitige Sitzungsabbruch ist am tatsächlich installierten
Produktionscode deterministisch reproduziert. Callback und Transport erfassen
ihre Zeitwerte vor dem gemeinsamen Peer-Lock. Ein anderer Ausführungspfad
kann das Budget bereits mit einem neueren Wert aktualisieren, bevor der erste
Pfad den Lock erhält. Sein älterer Beobachtungswert löst dann fälschlich die
dauerhafte Zeitrücksprungs-Sperre aus. Die Geräteuhr muss dafür nicht rückwärts
laufen. Das passt zu den gespeicherten frühen Sitzungsenden; eine Zeitspur des
konkreten ersten Liveauslösers lag in der alten Firmware nicht vor.

Die Korrektur erfasst die für Budgetentscheidungen verwendete aktuelle Zeit
unter demselben Peer-Lock wie den Budgetzugriff. Originale Callback- und
Dispatchzeitwerte bleiben erhalten; die tatsächliche Callbackzeit wird
weiterhin in Capture, Queue und Drahtformat übernommen. Für die verzögerte
Peerzulassung zählt auch das Warten auf den Lock zur unveränderten maximalen
Capture-Altersgrenze von 100000 µs.

Der neue Test injiziert genau eine andere Produktionszulassung zwischen
Zeitbeobachtung und Lockübernahme. Er verwendet die echten Peer-/Budgetpfade,
reale native Mutexes und eine monoton fortschreitende SDK-Testuhr. Die finale
Testfassung scheitert mit dem bytegleichen installierten alten `csi_peer.c`
an der unerwartet beendeten Sitzung. Nach der Korrektur bestehen Paar- und
Mesh-Verschachtelungen in beiden Richtungen, die verzögerte Zulassung,
100000/100001-µs-Grenzen und das genaue 900-s-Ende. Ein echter Rücksprung der
Budgetuhr beendet die Sitzung weiterhin. Die eigentlichen Budgetregeln in
`rx_budget.c` wurden nicht geändert.

Die C-Suite mit ASan/UBSan und 69 Python-Tests bestanden. Zwei unabhängig
gebaute Firmwaredateien waren bytegleich. Protokoll, Masken, Raten, Peerquoten,
900-s-Grenze, NVS-Schema und Partitionslayout wurden bei dieser Korrektur
beibehalten. Offline-Regressionen ersetzen keine Live-Peer-Qualifikation;
die getrennten Messberichte vom [27.](rx-unified-tag14-result-20261027.md) und
[29. Oktober](rx-unified-tag14-result-20261029.md) dokumentieren die Livebefunde.
