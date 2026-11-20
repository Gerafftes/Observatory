# Forschungsquellen

Die [Quellensammlung zu WiFi-Sensing und Personenerfassung](rf-sensing-sources.md) enthält 46 Quellen mit thematischer Einordnung. Die [Quellen zum TX/RX-Arbeitsplan](rf-sensing-sources.md#quellen-zum-txrx-arbeitsplan) bündeln die Primärbelege zu Frame-Zuordnung, RX-Uhren, TX1-Sendefehlern und der Radarprüfung für Tag 15.

| Thema | Einstieg | Nutzen und Grenze |
| --- | --- | --- |
| Ausgangsprojekt | [RuView](https://github.com/ruvnet/RuView) | Herkunft des verwendeten und angepassten Software-Snapshots; dessen Funktionen sind nicht automatisch für Observatory validiert. |
| WLAN und ESP32 | [ESP-NOW-Dokumentation für ESP-IDF 5.4.1](https://docs.espressif.com/projects/esp-idf/en/v5.4.1/esp32/api-reference/network/esp_now.html) | Grundlagen zu Transport und Callbacks; eine Sendebestätigung ist kein CSI- oder Positionsnachweis. |
| Diskrete Ortung | [From CSI to Coordinates](https://www.mdpi.com/1999-5903/17/9/395) | Forschungsbeispiel für rasterbasierte Klassifikation; Ergebnisse sind nicht auf unseren Aufbau übertragbar ohne eigene Prüfung. |
| Pose | [DensePose From WiFi](https://arxiv.org/abs/2301.00250) | Forschung zu menschlicher Körperoberfläche aus WiFi; kein Nachweis einer entsprechenden Observatory-Funktion. |
| Radar | [HLK-LD2450-Herstellerunterlagen](https://www.hlktech.net/index.php?id=1262) | Gerätegrundlage; für den eigenen Aufbau sind Raumtransformation und Genauigkeit gesondert zu prüfen. |

Primärarbeiten, technische Dokumentation und eigene Messberichte haben unterschiedliche Aussagen und Grenzen. Die [eigenen Ergebnisse](results.md) belegen nur die dort beschriebenen Aufbauten und Versuche. Der Stand der Quellenprüfung ist in der Quellensammlung dokumentiert.
