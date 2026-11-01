<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
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

Die gesamte C-Suite mit ASan/UBSan sowie 69 Python-Tests bestanden. Eine enge
Jev-Quellflussprüfung unterstützte die serialisierte Budgetzeit und den Erhalt
der Capturezeit. Die Alters-/Rücksprungsfrage blieb dort unentschieden und
wurde anhand der unveränderten Budgetquelle und tatsächlich ausgeführten
Grenz-/Rücksprungtests selbst geprüft. Jev zertifiziert weder Tests noch
Hardware oder Freigaben.

Zwei unabhängige Builds mit dem sauberen gebundenen ESP-IDF 5.4 und dem
gepinnten Compiler lieferten bytegleiche **909520 B** große Apps:

- App-SHA256: `20550def4f3649620bcd39d397a62c4ccf39d064baa4c1b89677648a8691bdb2`
- Image-Digest: `e2372e5cdf59771d487e5e765adfd9345c1f7dd6607ad70313475a1544dfb8c0`
- Quellmanifest: `62e058d852e75e47a321e0efd5daa861733ae3c5979884835a9bc34089b581b7`
- Partitionsdatei unverändert: `67222c257c0477501fd4002275638dc4262b34eb68235b8289fb1337054d322b`

Gegenüber dem installierten Kandidaten änderten sich im Produktionsmanifest
nur `main/csi_peer.c` und dessen Header. Protokoll, Masken, Raten, Peerquoten,
900-s-Grenze, NVS-Schema, Bootloader und Partitionslayout bleiben erhalten.
Dies ist eine Offline-Freigabebasis, noch kein Live-Peer- oder Tag14-PASS.

Die privaten Belege liegen unter
`PRIVATE_EVIDENCE/rx-unified-peer-clock-fix-20261027-run01/`:
Vorher-/Nachherquellen und Patch, negativer Test des installierten Codes,
C-/Python-Protokolle, Quellflussprüfung, `OFFLINE-VERIFIED.private.json` sowie
`release-candidate-v3/` mit beiden Builds, Manifesten und Ressourcenbericht.
Die neue gerätegebundene Installationsvorbereitung unter
`rx-unified-live-20261027-run01/amendment-02/` erhält vorhandene Profile
(RX1/RX4 Generation 3, RX2/RX3 Generation 1), verlangt frische doppelte
Vollsicherungen und behält sämtliche Schutz-/Rückleseprüfungen. Die bestehende
Schreibfreigabe für den alten Hash ist verbraucht; die Ergänzung für den neuen
Hash wurde gesondert angefragt. Das gemeinsame Ende bleibt 23:10 Uhr.
