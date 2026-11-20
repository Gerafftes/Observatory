# Messblatt

Leere Vorlage für eine einzelne Messung. Ausgefüllte Blätter bleiben privat.
Geräteidentitäten, Netzwerkdaten, lokale Dateipfade und Angaben zu Personen
nur in der vorgesehenen privaten Messablage erfassen; für öffentliche Berichte
anonyme Rollen und bereinigte Ergebnisse verwenden. Der Transportstatus allein
belegt weder Messqualität noch eine gemeinsame Geräteabnahme.

## Allgemein

| Feld | Wert |
|---|---|
| Mess-ID | |
| Datum | |
| Startzeit | |
| Endzeit | |
| Raum | |
| Versuchsart | A / B |
| Beschreibung | |

## Hardware

| Gerät | Rolle | Position | Status |
|---|---|---|---|
| ESP32 #1 | TX | | |
| ESP32 #2 | RX1 / Node 1 | | |
| ESP32 #3 | RX2 / Node 2 | | |
| ESP32 #4 | RX3 / Node 3 | | |
| ESP32 #5 | RX4 / Node 4 | | |
| mmWave 1 / ESP32-C3 + HLK-LD2450 | getrennte Referenz | | |
| mmWave 2 / ESP32-C3 + HLK-LD2450 | getrennte Referenz | | |

## Software

| Feld | Wert |
|---|---|
| RuView-Version/Commit | |
| Server-Konfiguration (bereinigte Referenz; vollständiger Befehl privat) | |
| UDP-Port | |
| WLAN-SSID (nur privat) | |
| WLAN-Kanal | |
| TX-MAC (nur privat) | |
| Filter-MAC aktiv | ja / nein |
| Edge-Tier | |

## Messsituation

| Feld | Wert |
|---|---|
| leerer Raum | ja / nein |
| Person vorhanden | ja / nein |
| Anzahl Personen | |
| Position / Zone | |
| Bewegung | keine / langsam / stark |
| Atmungstest | ja / nein |
| Unabhängige Referenz für Atmung, falls vorhanden | |

Die LD2450-Bewegungsreferenz ist hier kein Nachweis für Atmung. Eine
Atmungsaussage benötigt eine dafür geeignete, gesondert dokumentierte Referenz.

## Beobachtungen

- …
- …
- …

## Erste Qualitätsprüfung

| Prüfung | Ergebnis |
|---|---|
| RX1 sendet Daten | ja / nein |
| RX2 sendet Daten | ja / nein |
| RX3 sendet Daten | ja / nein |
| RX4 sendet Daten | ja / nein |
| mmWave 1 sendet Daten | ja / nein / nicht geprüft |
| mmWave 2 sendet Daten | ja / nein / nicht geprüft |
| Gemeinsame Abnahme beider mmWave-Knoten belegt | ja / nein / offen |
| Bewegung sichtbar | ja / nein / unklar |
| Atmung sichtbar | ja / nein / unklar |

## Dateien

| Bereinigte Artefakt-ID (vollständiger Ablageort nur privat) | Inhalt |
|---|---|
| | |
| | |
