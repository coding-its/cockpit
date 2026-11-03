# Bedienung

Kurzanleitung für die wiederkehrenden Aufgaben im Cockpit.

## Datenbank

Kunden, Geräte, Zeiten und Berichte liegen in **PostgreSQL**. Die Verbindungsdaten stehen in
`db.json` und sind in der Anwendung änderbar: **Einstellungen → Datenbank**.

| Feld | Beispiel | wofür |
| --- | --- | --- |
| Server | `192.168.99.88` | Rechner, auf dem PostgreSQL läuft |
| Port | `5432` | Standard bei PostgreSQL |
| Datenbank | `wartungscockpit` | muss vorher angelegt sein |
| Benutzer / Passwort | `esc24` / … | eigenes Konto, nicht `postgres` |

Aktuell: zentrale Datenbank auf `192.168.99.88` (PostgreSQL 16, Ubuntu), Konto `esc24`.

**Verbindung testen** prüft, ob der Server erreichbar ist, ohne etwas zu speichern. Gespeichert
wird nur, wenn der Test erfolgreich war. Die neue Verbindung gilt **nach einem Neustart** —
in der Desktop-App steht dafür ein Knopf bereit.

### Neuen Arbeitsplatz einrichten

1. `Wartungscockpit_0.1.4_x64_de-DE.msi` installieren.
2. **Starten.** Ohne hinterlegte Verbindung öffnet sich das Fenster **„Datenbank einrichten"**:
   Server, Port, Datenbank, Benutzer und Passwort eintragen, **Verbindung testen**, dann
   **Speichern und starten**. Die Anwendung prüft vor dem Speichern und startet danach neu.
3. Die `.env` mit den NinjaOne-Zugängen aus dem Projektordner nach
   `%APPDATA%\Wartungscockpit\` kopieren. Ohne sie läuft NinjaOne im Mock-Modus – alles
   andere funktioniert, nur die echten Geräte- und Patchdaten fehlen.
   Alternativ als Administrator anmelden und die Angaben unter **Einstellungen → NinjaOne**
   eintragen.

**Von einem eingerichteten Rechner abschauen** geht schneller: `db.json` und `.env` aus
`%APPDATA%\Wartungscockpit\` kopieren. Das Passwort steht in `db.json` im Klartext – die Datei
nicht weitergeben, außer an die eigenen Arbeitsplätze.

### Aktualisieren

Das MSI braucht **Administratorrechte** – Windows fragt beim Doppelklick danach. Eine neue
Fassung lässt sich normalerweise einfach darüberinstallieren; die Verbindung in
`%APPDATA%\Wartungscockpit\` bleibt dabei erhalten.

**Wenn die Installation mit 1603 abbricht**, ist meist die alte Fassung im Weg:

1. **Einstellungen → Apps → Installierte Apps** → Wartungscockpit → **Deinstallieren**.
2. Neue MSI installieren.

`db.json`, `.env` und das Log liegen in `%APPDATA%\Wartungscockpit\` und werden beim
Deinstallieren **nicht** entfernt – nach dem Neuinstallieren startet die Anwendung also sofort
mit den bisherigen Einstellungen.

### Wenn sich die Anwendung nicht öffnet

Es gibt jetzt eine Logdatei, die genau das beantwortet:

```
%APPDATA%\Wartungscockpit\wartungscockpit.log
```

Dort stehen Version, gefundene WebView2-Version, die verwendete Datenbankverbindung und jeder
Fehler mit Klartext. Beim Melden eines Problems diese Datei mitschicken.

Häufige Ursachen:

| Symptom | Ursache |
| --- | --- |
| Fenster „Datenbank einrichten" erscheint | keine oder falsche Verbindung – normal beim ersten Start |
| Gar kein Fenster, Log fehlt | Absturz vor dem Start; meist fehlt **WebView2** (im Log: `webview2=nicht gefunden`) |
| Gar kein Fenster, Log vorhanden | Meldung in der Logdatei lesen – meist Datenbank oder Berechtigungen |

WebView2 bringt Windows 11 mit. Auf älteren Windows-10-Rechnern installiert das MSI es
nachträglich – dafür braucht der Rechner beim Installieren einmal Internetzugang.

### Umzug auf einen anderen Server

Datenbank dort anlegen und `esc24` zum Eigentümer machen:

```sql
CREATE DATABASE wartungscockpit OWNER esc24 ENCODING 'UTF8';
```

Das Schema legt die Anwendung beim ersten Start selbst an. Daten übernehmen mit
`scripts/import-sqlite-to-postgres.py` (aus der alten SQLite-Datei) oder `pg_dump`/`pg_restore`
(von PostgreSQL zu PostgreSQL). Danach in der App die neue Adresse eintragen, testen, speichern,
neu starten.

**Warum eine Datei und nicht die Datenbank:** Man muss sich verbinden können, bevor man dort
etwas lesen kann. Die Zugangsdaten liegen deshalb neben dem Programm. Wer die Datei lesen kann,
kommt auch mit einem Datenbankwerkzeug an die Daten – für ein Netz, in dem sich alle vertrauen,
ist das vertretbar; für strengere Anforderungen wären eigene Datenbankkonten je Person nötig.

## Kunden aus der Übersicht ausblenden

**Kunden → Ansicht wählen**

Oben stehen drei Ansichten mit Zähler:

| Ansicht | Bedeutung |
| --- | --- |
| **Fokus** | Nur die Kunden, die in Übersicht und Kennzahlen erscheinen |
| **Ausgeblendet** | Die stillgelegten „Einzelkämpfer", weiterhin voll verwaltet |
| **Alle** | Gesamtbestand |

In jeder Zeile ganz rechts sitzt in der Spalte **Übersicht** ein Augen-Symbol:

- **Auge sichtbar** = Kunde ist im Fokus. Klick blendet ihn aus.
- **Auge durchgestrichen** = Kunde ist ausgeblendet. Klick holt ihn zurück.

Jede Umschaltung landet im Audit-Trail, ist also nachvollziehbar.

**Was Ausblenden bedeutet:** Der Kunde verschwindet aus Dashboard, Handlungsbedarf und
Kennzahlen. Geräte, Sites, Zeiten und Berichte bleiben vollständig erhalten – die
Seiten **Zeiten** und **Abrechnung** zeigen weiterhin alle Kunden, damit keine erbrachte
Leistung aus dem Monatsbericht fällt.

**Vorbelegung:** Beim ersten Live-Sync wurden Kunden mit fünf oder mehr Geräten in den
Fokus gesetzt (17 von 39). Das ist anpassbar und nur ein Startwert.

## Web-Kunde anlegen

**Websites → Web-Kunde anlegen**

Name, Kürzel, Stundensatz, Ansprechpartner, E-Mail, Telefon, Notizen. Der Kürzel wird aus dem
Namen gebildet, wenn er leer bleibt.

**Wichtig:** WordPress-Kunden sind ein eigener Stamm. Sie tauchen bewusst **nicht** bei den
NinjaOne-Kunden auf – andere Datenquelle, eigene Zugangsdaten, eigene Zeiten. In der Abrechnung
laufen beide zusammen.

## Web-Kunde löschen oder deaktivieren

**Websites → Web-Kunde** – oben rechts in der Kundenzeile zwei Symbole:

- **Auge** – deaktivieren oder wieder aktivieren. Ein deaktivierter Kunde verschwindet aus
  Websites, Bericht und Abrechnung, seine Daten bleiben vollständig. Das ist der übliche Weg
  für Kunden, die nicht mehr betreut werden.
- **Papierkorb** – endgültig löschen, mit Rückfrage. Entfernt gleichzeitig Websites,
  Zugangsdaten und Agent-Aktionen.

**Schutz der Abrechnung:** Hat der Kunde Zeiteinträge, verweigert das Cockpit das Löschen und
schlägt Deaktivieren vor. Zeiten verschwinden also nur, wenn man sie vorher ausdrücklich
einzeln löscht – ein versehentlicher Klick auf „löschen" kann keine Rechnung zerstören.

## Website bearbeiten und löschen

**Websites → Web-Kunde → Website-Karte → Bearbeiten**

Dasselbe Formular wie beim Anlegen, nur gefüllt. Adresse, Bezeichnung, Umgebung, Notizen,
Agent-Zugang und Login-Daten lassen sich ändern.

**Passwörter:** Die drei Passwortfelder (Application Password, Agent-Secret, Login-Passwort)
bleiben beim Bearbeiten leer und zeigen den Hinweis „leer lassen = unverändert". Wer ein Feld
leer lässt, behält das gespeicherte Passwort; wer eines ausfüllt, ersetzt es. Gespeicherte
Passwörter werden nie ins Formular zurückgegeben – so landen sie nicht im Verlauf.

**Textfelder leeren:** Ein geleertes Feld für Bezeichnung, Notizen, Agent-Benutzer,
Login-Adresse und so weiter wird als „nicht gesetzt" gespeichert. Damit lässt sich auch etwas
entfernen, nicht nur ändern.

**Löschen:** Der Knopf **Löschen** fragt vorher nach. Entfernt werden die Website, ihre
Zugangsdaten und ihr Agent-Zugang; die zugehörigen Agent-Aktionen verschwinden mit. **Zeiten
bleiben erhalten** – sie verlieren nur den Bezug zur Website, damit die Abrechnung nicht
rückwirkend Lücken bekommt. Das steht auch in der Rückfrage.

Wer eine Website mit anderen Daten neu aufsetzen will, kann sie löschen und neu anlegen; der
übliche Weg ist aber Bearbeiten, weil dann ID, Zeiten und Historie erhalten bleiben.

## Website hinzufügen

**Websites → Web-Kunde öffnen → „Website hinzufügen"**

Felder:

| Feld | Woher |
| --- | --- |
| Internetadresse | `www.example.de` oder vollständig; `https://` wird ergänzt |
| Bezeichnung | frei, z. B. „Hauptseite" |
| Umgebung | Produktiv / Staging / Lokal |
| Agent-Benutzer | WordPress-Benutzer, z. B. `wartung-cockpit` |
| Application Password | Benutzerprofil → Application Passwords |
| Agent-Secret | `WC_MAINT_SHARED_SECRET` aus der `wp-config.php` |
| Login-Adresse | z. B. `.../wp-login.php` für den Schnellzugriff |
| Login-Benutzer / -Passwort | dein eigener Zugang, nicht der Agent |
| Hinweis zum Login | z. B. „Passwort im Passwortmanager" |
| Notizen | Hoster, Besonderheiten, Backup-Weg |

**Zum Login:** Der Button **Login** öffnet die Anmeldeseite in einem neuen Fenster. Ein iFrame
wäre technisch möglich, scheitert aber bei den meisten Seiten an `X-Frame-Options` – WordPress
und die meisten Hoster verbieten das Einbetten. Deshalb `target="_blank"`. Benutzername und
Passwort stehen direkt daneben mit Kopier-Button.

**Zum Login-Speicher:** Die Zugangsdaten liegen unverschlüsselt in der lokalen Datenbank. Für
diesen Betrieb ist das eine bewusste Entscheidung: Das Cockpit läuft nur im internen Netz, nicht
im Internet. Wer Zugriff auf die Datenbankdatei hat, hat ohnehin Zugriff auf alles. Kommt das
Cockpit später ins Internet oder auf einen Server außerhalb eures Hauses, gehört eine
Verschlüsselung mit Master-Key dazu.

Voraussetzungen auf der Site:

1. Mu-Plugin unter `wp-content/mu-plugins/wartungscockpit-agent.php`
2. `WC_MAINT_SHARED_SECRET` in der `wp-config.php`
3. `WP_ENVIRONMENT_TYPE` = `local` (bei HTTP) oder HTTPS
4. `.htaccess` mit `RewriteBase` und `HTTP_AUTHORIZATION`-Weiterleitung

Nach dem Speichern zeigt die Karte Version und Update-Zahl, sobald **Health** einmal gelaufen
ist. Die drei Buttons pro Website:

| Button | Wirkung |
| --- | --- |
| **Health** | Liest Versionen und offene Updates, prüft die Erreichbarkeit |
| **Plugins** | Dry-Run, dann echtes Update, danach Health-Check |
| **Core** | Core-Update mit Health-Check |

**Backup-Bestätigung:** Vor Plugin- oder Core-Updates muss ein verifiziertes Backup hinterlegt
sein. Ohne Bestätigung bricht die Ausführung mit klarer Meldung ab.

Jede Aktion erscheint unter **Letzte Aktionen** mit Ergebnis, Dauer und Version. Zusätzlich
entsteht ein Maschinenzeit-Eintrag, der den Aufwand belegt (nicht abrechenbar).

## Remote-Verbindung zu einem Gerät

**Kunden → Kunde → Geräte → Zeile → „Remote"**

Der Link öffnet die Geräteseite in der NinjaOne-Oberfläche. Dort startet die
Remote-Session (Splashtop/TeamViewer) wie gewohnt.

**Warum dieser Umweg?** NinjaOne stellt für den Session-Start **keine öffentliche API**
bereit. Die Tokens kommen aus einer privaten Hersteller-API mit IP-Allowlist; sie werden
on demand erzeugt und verfallen schnell. Ein direkter Start aus dem Cockpit ist damit
nicht möglich – der Deep-Link ist der ehrliche Kompromiss: ein Klick statt Suchen im
Gerätebestand.

**Praktisch:** Cockpit im einen Tab (Timer läuft), NinjaOne im anderen. Die Zeiterfassung
läuft weiter, während die Session offen ist.

### TeamViewer direkt

**Kunden → Kunde → Geräte → Anzeigen → TeamViewer**

Hat ein Gerät eine TeamViewer-ID, erscheint im Panel neben **Remote** ein zweiter Knopf
**TeamViewer**. Der braucht keinen Umweg über NinjaOne: er ruft das von TeamViewer
registrierte URL-Schema `teamviewer10://control?device=<ID>` auf, TeamViewer startet und
verbindet sofort. Ein Klick, keine Suche nach der ID.

**ID hinterlegen:** im Geräte-Panel in der Kachel **TeamViewer** auf „hinterlegen" klicken,
Nummer eintragen, Enter. Leerzeichen und Bindestriche sind erlaubt – gespeichert werden nur
die Ziffern, weil genau die im Link stehen müssen. Leer speichern löst die Verknüpfung wieder.
Abschalten geht über „ändern".

**Woher die ID kommt:** NinjaOne liefert sie nicht mit, sie wird also einmal pro Gerät von
Hand gepflegt. Wer sie nicht zur Hand hat, fragt beim Kunden nach oder liest sie in der
Sitzung ab. Der Link funktioniert nur auf Rechnern, auf denen TeamViewer installiert ist –
beim ersten Mal fragt Windows einmalig nach, ob TeamViewer geöffnet werden darf.

## Handlungsbedarf im Kunden

**Kunden → Kunde → Geräte**

Ist ein Kunde in der Kundenübersicht mit **Handlungsbedarf** markiert, steht im Kundendetail
über der Geräteauswahl ein roter Kasten **Handlungsbedarf**. Er listet genau die Geräte, die
dazu zählen, und nennt den Grund:

- „26 Patches offen" – offene Betriebssystem- oder Software-Patches
- „Bitdefender … – Signaturen veraltet" – Antivirus inaktiv oder veraltet

Ein Klick auf eine Zeile öffnet das Geräte-Panel, drei Klicks auf das Gerät und der Timer läuft.
Steht bei der Überschrift „12 mit Handlungsbedarf" neben der Gerätezahl, sind 12 der 23 Geräte
betroffen.

**Was zählt als Handlungsbedarf:** ein Gerät mit offenen Patches **oder** mit einer
Antiviruswarnung, dazu WordPress-Sites im Zustand „Beobachten" oder „Kritisch". Die Zahl in der
Kundenübersicht und die Liste im Kundendetail kommen aus derselben Bedingung und stimmen
deshalb überein.

Die Liste ist auf 15 rem begrenzt und scrollt intern – bei einem Kunden mit vielen Geräten
bleibt die Geräteauswahl darunter erreichbar.

## Gerät ansehen und Zeit erfassen

**Kunden → Kunde → Geräte**

Oben ein Dropdown mit allen Geräten des Kunden und ein Button **Anzeigen**. Darunter öffnet sich
das Geräte-Panel:

- **Patchstand**, **Antivirus**, **Status** (online/offline), **letzter Kontakt**, **TeamViewer**
- bei gemeldeten Patches ein aufklappbarer Kasten mit den Einträgen, offene zuerst
- Arbeitshinweis: erst Zeit starten, Aufgabe wählen, ausführen, Kommentar schreiben
- Aktionsreihe in dieser Reihenfolge:
  1. **Timer starten** (bzw. laufende Zeit + Stop)
  2. **Remote** – öffnet das Gerät in NinjaOne, dort startet die Remote-Session
  3. **TeamViewer** – nur wenn eine ID hinterlegt ist, startet die Session direkt
  4. **Patch-Scan** – läuft direkt im Panel, Ergebnis erscheint darunter
  5. **Script-Test** – ebenso (eingebaute Aktion, Standard DNS-Cache leeren)
- **Zeit auf diesem Gerät** – Tätigkeit und Kommentar; gestartet wird über den Button oben

Schnellaktionen wechseln **nicht** die Seite. Fortschritt und Ergebnis erscheinen als Kasten direkt
unter der Aktionsreihe, inklusive der Ausgabe, die NinjaOne zurückmeldet. Im Hintergrund entsteht
trotzdem ein Wartungslauf mit Nachweis – den findest du unter **Wartungen** und im Monatsbericht.

Der Timer läuft in der Kopfleiste weiter, auch wenn du den Kunden wechselst. Beim Stoppen
erscheint die Zeit unter „Letzte Zeiten" mit Gerätename und Kommentar.

## Zeit erfassen

- **Website:** **Websites → Web-Kunde → Zeit erfassen**. Links Website (optional) und
  Tätigkeit, rechts der Kommentar als mehrzeiliges Feld. Die Tätigkeit schlägt gängige
  Arbeiten vor, lässt sich aber frei beschreiben.
  - **Timer starten** misst die Zeit live; die laufende Zeit steht auch oben rechts.
  - **Speichern** trägt die Arbeit nachträglich mit der Dauer in Minuten ein – für alles,
    was schon erledigt ist. Beides landet mit Kommentar beim Kunden und im Monatsbericht.
- **Wartungslauf:** bei der Aufgabe auf „Timer".
- **Einsatz:** gleiches Prinzip unter **Einsätze**.
- **Manuell:** **Zeiten → Zeit erfassen** für Nachträge.
- **Maschinenzeit** entsteht automatisch aus Job- und Agent-Laufzeiten, nicht abrechenbar.

## Website-Vorschau im Web-Kunden

Im Web-Kunden steht neben den Websites eine Karte **Website-Vorschau**: die Seite klein im
Rahmen, darunter der Hinweis, dass ein Klick sie öffnet und die Zeit startet.

- **Klick auf die Vorschau** öffnet die Seite in einem neuen Tab und startet den Timer
- Mit dem **Augensymbol** in der Kopfzeile der Vorschau blendest du sie aus, wenn sie störst
- Bleibt der Rahmen leer, verbietet die Seite das Einbetten (`X-Frame-Options`) – das passiert
  bei den meisten Kunden-Websites. Der Klick funktioniert trotzdem, die Seite öffnet normal.

Darunter stehen **Letzte Aktionen** und **Zeiten mit Kommentar** nebeneinander.



## Kommentar beim Stoppen

**Sobald ein Timer endet, fragt das Cockpit nach dem Kommentar.** Das gilt für jeden Weg:

- **Stop** in der roten Leiste oben rechts
- **Stop** im Geräte-Panel
- **Geräte-Panel schließen** (das X) – beendet die Arbeit am Gerät und stoppt den Timer
- **Stop** im Web-Kunden

Der Dialog zeigt oben, was gestoppt wurde: Tätigkeit, Dauer und Kunde. Der Kommentar landet
direkt am eben beendeten Zeiteintrag und damit im Monatsbericht. Mit **Strg + Enter** speicherst
du, **Ohne Kommentar** schließt ohne Text.

Praktisch für den Ablauf: Gerät öffnen, Timer startet von selbst, arbeiten, Panel schließen –
Kommentar schreiben, fertig.

**Website bearbeiten** fragt ebenfalls nach dem Kommentar: Sobald du im Web-Kunden eine Website
speicherst, öffnet sich derselbe Dialog – mit Dauer-Feld, wenn kein Timer läuft. Läuft schon ein
Timer für diesen Kunden, kommt stattdessen der Hinweis „Kommentar wird beim Stoppen des Timers
abgefragt", und der Dialog erscheint beim Stopp. So wirst du in beiden Fällen erinnert, ohne dass
sich zwei Fenster überlagern.

## Kommentare zu Zeiten

Jeder Zeiteintrag hat ein Kommentarfeld. Es ist an drei Stellen editierbar und wird sofort
gespeichert, sobald das Feld den Fokus verliert:

- im Web-Kunden unter **Zeiten mit Kommentar**
- in der Spalte **Kommentar** unter **Zeiten**
- beim Start eines Timers (Feld „Kommentar")

Der Kommentar landet im CSV-Export in der Spalte **Beschreibung** – also genau dort, wo die
Kollegin deines Chefs ihn für die Rechnung braucht. Datum und Uhrzeit stehen immer in den
Spalten davor.

## Einsätze (spontane Arbeiten)

**Einsätze → Einsatz anlegen**: Kunde, Titel, Priorität, Eingangskanal, Bearbeiter,
Fälligkeit, optional Gerät. Danach läuft derselbe Workflow wie bei Wartungen: Timer,
Dokumentation, Protokoll.

## Einstellungen

**Einstellungen** in der linken Leiste – nur für Administratoren sichtbar. Zwei Reiter:

**Benutzer** – alles zur Anmeldung, siehe den Abschnitt darunter.

**Stundensätze** – der Satz, der greift, wenn im Vertrag eines Kunden nichts gepflegt ist
(Standard 126,05 € netto), und der Umsatzsteuersatz. Das Formular zeigt daneben sofort den
Bruttobetrag. Kunden mit eigenem Vertragssatz behalten ihren Satz; im Bericht steht bei den
anderen „Standard".

Die Werte gelten sofort für Übersicht, Bericht und Abrechnung – ein Neustart ist nicht nötig.
Jede Änderung landet als „Abrechnungseinstellungen geändert" im Audit-Trail.

## NinjaOne-Zugang ändern

**Einstellungen → NinjaOne** (nur als Administrator). Dort stehen alle Angaben, die sonst in
der `.env` von Hand gepflegt werden müssten:

| Feld | wofür |
| --- | --- |
| **Mock-Modus** | ein: keine Verbindung, Demo-Daten. aus: Echtbetrieb |
| **Server-Adresse** | `https://eu.ninjarmm.com` (EU) oder `https://app.ninjarmm.com` (US) |
| **Client-ID / Client-Secret** | Client-Credentials-App aus NinjaOne → Administration → Apps → API |
| **Berechtigungen (Scope)** | `monitoring management` reicht für alles außer Scripts |
| **Benutzer-App** | zweite App vom Typ *Webanwendung*, nur für Script-Ausführung |

**Verbindung testen** fragt bei NinjaOne ab, wie viele Organisationen sichtbar sind – ohne
etwas zu speichern. **Speichern** prüft zuerst und schreibt nur, wenn die Verbindung steht; ein
Tippfehler im Secret kann die laufende Verbindung also nicht kaputt machen. Im Live-Modus
verweigert das Formular das Speichern, solange Client-ID oder Secret fehlen.

Geheimnisse werden **nie zurückgegeben** – das Formular zeigt nur, ob eines hinterlegt ist.
Leer lassen heißt: bisheriges behalten.

**Wo die Datei liegt:** Der Pfad steht direkt im Formular. In der installierten Anwendung ist es
`%APPDATA%\Wartungscockpit\.env`, beim Entwicklungsbetrieb die `.env` im Projektordner.
Gespeichert wird nur die geänderte Zeile; Kommentare und andere Einträge bleiben erhalten.

**Nach dem Speichern** gilt der neue Zugang erst nach einem Neustart. In der Desktop-App steht
dafür ein Knopf **„Jetzt neu starten"** direkt neben dem Hinweis; im Browser muss die API von
Hand neu gestartet werden.

**Im Installer stecken keine Zugangsdaten.** Wer das MSI weitergibt, gibt keine Zugänge weiter –
jeder Arbeitsplatz braucht seine eigene `.env` (oder die Angaben einmal im Formular).

## Benutzer und Anmeldung

**Anmeldung:** Eigene Seite mit Benutzername und Passwort. Die Sitzung läuft 14 Tage und endet
beim Abmelden sofort. Unten links in der Seitenleiste stehen Name und Rolle, daneben der
Abmelden-Knopf.

**Benutzername oder Anzeigename:** Die Anmeldung akzeptiert beides. `admin` und `Administrator`
führen zum selben Konto, solange der Anzeigename eindeutig ist. Groß- und Kleinschreibung ist
beim Anzeigenamen egal.

**Erster Start:** Es wird automatisch ein Konto `admin` angelegt. Das Startpasswort steht
**einmalig im Serverlog** (`api.log`) – dort suchen, notieren, danach ändern. Alternativ vor dem
ersten Start setzen:

```env
ADMIN_USERNAME=admin
ADMIN_DISPLAY_NAME=Administrator
ADMIN_PASSWORD=dein-startpasswort
```

## Passwort vergessen

Kein Datenbankwerkzeug nötig. Im Projektordner:

```cmd
apps\api\target\debug\wartungscockpit-api.exe --reset-admin-password
```

Ausgabe:

```
  Benutzername: admin
  Neues Passwort: <zufällig erzeugt>
```

Das neue Passwort gilt sofort, alle Sitzungen des Kontos sind beendet, das Konto ist wieder
aktiv. Anderer Benutzer:

```cmd
... --reset-admin-password s.wolf
```

Das Kommando funktioniert auch, während die App läuft – es braucht nur die Datenbankdatei.
Der Vorgang landet als „Passwort zurückgesetzt (Kommandozeile)" im Audit-Trail.

**Rollen:**

| Rolle | darf |
| --- | --- |
| **Administrator** | alles, inklusive Benutzerverwaltung |
| **Techniker** | arbeiten: ausführen, Zeiten erfassen, Kunden und Websites pflegen |
| **Nur Lesen** | alles sehen, nichts ändern – gedacht für Praktikanten |

**Benutzer anlegen:** **Benutzer → Benutzer anlegen** (nur als Administrator sichtbar). Das
Startpasswort muss die Person beim ersten Login selbst ändern.

**Passwort vergessen:** Administrator setzt unter **Benutzer → Passwort** ein neues. Alle
Sitzungen dieses Kontos werden dabei beendet.

**Sicherungen im Code:** Der letzte aktive Administrator kann nicht herabgestuft oder deaktiviert
werden. Das eigene Konto lässt sich nicht deaktivieren. Wer deaktiviert wird, fliegt sofort aus
allen Sitzungen. Passwörter liegen als Argon2id-Hash in der Datenbank, Sitzungstokens nur als
SHA-256-Hash – der Klartext verlässt den Server nie.

## Mehrere Personen gleichzeitig

Jede Person hat ihren **eigenen Timer**. Zeitgleich messen ist ausdrücklich erlaubt: Du siehst
in der Kopfleiste nur deine eigene laufende Zeit, und ein Start beendet nie den Timer eines
Kollegen.

Unter **Zeiten** steht über den Kennzahlen eine schmale Leiste **„Gerade in Arbeit"**, sobald
jemand anderes misst – mit Name, Tätigkeit und Kunde. Sie frischt alle 30 Sekunden auf. Wer
gerade nicht arbeitet, sieht die Leiste gar nicht.

Alle Zeiteinträge bleiben für alle sichtbar, mit **Name in der Spalte**. Das ist Absicht: deine
Chefin muss am Monatsende sehen, wer welche Stunden geleistet hat, und die Abrechnung fasst die
Zeiten eines Kunden zusammen, unabhängig davon, wer sie erbracht hat.

**Was das für den Betrieb bedeutet:** Die Daten liegen in **PostgreSQL**, einem Dienst, der
Verbindungen annimmt. Jede Person hat das Cockpit auf ihrem eigenen Rechner installiert; die
Anwendung verbindet sich direkt mit der Datenbank – es gibt keinen dazwischenliegenden
Webserver.

- **Datenbank:** PostgreSQL 18 auf einem Rechner im Netz, Datenbank `wartungscockpit`,
  eigenes Konto `cockpit`. Im Moment läuft sie auf diesem Rechner (`127.0.0.1:5432`); für den
  Dauerbetrieb zieht sie auf einen Server um.
- **Cockpit:** als Anwendung auf jedem Arbeitsplatz (`Wartungscockpit_0.1.4_x64_de-DE.msi`).
  Nach der Installation `db.json` und `.env` nach `%APPDATA%\Wartungscockpit\` legen.
- **Umzug der Datenbank:** In der App unter **Einstellungen → Datenbank** die neue Adresse
  eintragen, Verbindung testen, speichern, App neu starten. Kein Neuinstallieren.

**Netzfreigabe:** Andere Rechner kommen nur an die Datenbank, wenn beides erlaubt ist:

1. In `pg_hba.conf` (Ordner `data` der PostgreSQL-Installation) eine Zeile fürs Firmennetz,
   z. B. `host all all 192.168.99.0/24 scram-sha-256`, danach den Dienst neu starten.
2. Eine eingehende Firewall-Regel für TCP 5432 auf dem Datenbankrechner.

Beides braucht Administratorrechte. Ohne diese Schritte funktioniert das Cockpit nur auf dem
Rechner, auf dem PostgreSQL läuft.

**Sicherung:** `pg_dump` reicht, kein Dateikopieren:
`pg_dump -h 127.0.0.1 -U cockpit -d wartungscockpit -F c -f cockpit-backup.dump`.
Am besten nach Feierabend und auf einem anderen Datenträger.

**Zugangsdaten:** Die Datenbank-Verbindungsdaten liegen auf jedem Arbeitsplatz in
`%APPDATA%\Wartungscockpit\db.json`. Wer sie liest, kommt mit einem Datenbankwerkzeug an alle
Zeiten – die Rolle „Nur Lesen" ist eine Konvention in der Oberfläche, keine Sperre in der
Datenbank. Für fünf Personen, die sich vertrauen, ist das vertretbar.

**Ein Punkt für später:** Die Verbindung zur Datenbank läuft unverschlüsselt (kein TLS). Im
eigenen Netz ist das üblich und funktioniert; wer mitschneiden kann, sieht aber auch die
Zeiten. PostgreSQL kann TLS mit einem internen Zertifikat – das wäre der nächste Schritt,
wenn das Cockpit dauerhaft läuft.

## Monatsbericht

**Bericht** in der linken Leiste öffnet den Leistungsnachweis für den gewählten Monat. Das ist
das Dokument, das am Monatsende an die Abrechnung geht – eine Seite pro Kunde, oben eine
Zusammenfassung über alle.

**Aufbau:**

1. **Zusammenfassung** – je Kunde eine Zeile: Vertrag, abrechenbare Stunden (mit „inkl."-Anteil
   aus dem Vertrag), Überhang, Stundensatz, Zeitbetrag, Pauschale und Summe. Darunter die
   Gesamtsumme und die nicht abrechenbare Zeit des Monats.
2. **Je Kunde ein Block** – Ansprechpartner und Vertrag, alle durchgeführten Wartungsläufe mit
   Aufgaben und Ergebnissen, bei Web-Kunden die Website-Aktionen (Health-Check, Plugin- und
   Core-Updates mit Vorher-/Nachher-Version), danach die Zeiten mit Datum, Uhrzeit, Tätigkeit,
   Kommentar und Gerätebezug.

**Abrechnungslogik:** Bei NinjaOne-Kunden wird nur der Überhang über die im Vertrag enthaltenen
Stunden berechnet, mal Stundensatz, plus Pauschale. Bei Web-Kunden zählt die gesamte
abrechenbare Zeit, weil es dort keine enthaltenen Stunden gibt. Nicht abrechenbare Einträge
tauchen im Bericht auf, gehen aber nicht in die Summe ein.

**Steuer:** Die Summen sind netto. Unter der Gesamtsumme stehen zusätzlich die Umsatzsteuer und
der Bruttobetrag; im Kundenblock steht beides nebeneinander („12,68 € netto · 15,09 € brutto").

**Standard-Stundensatz:** Ist im Vertrag kein Satz gepflegt, rechnet der Bericht mit
**126,05 € netto** (150,00 € brutto bei 19 % USt). In der Tabelle steht dann unter dem Satz klein
**Standard**, und unter der Tabelle nennt eine Zeile die betroffenen Kunden. Wer einen eigenen
Satz im Vertrag hat, wird immer damit gerechnet.

**Drucken je Kunde:** Jeder Kundenblock hat oben rechts den Knopf **Nur diesen Kunden drucken**.
Auf dem Papier steht dann nur dieser Kunde – mit Briefkopf, Monat, seinen Wartungsläufen,
Website-Aktionen und Zeiten. Zusammenfassung und die anderen Kunden entfallen. Praktisch, wenn
ein einzelner Kunde seinen Nachweis braucht und nicht den ganzen Monatsstapel.

**Kunden ohne Aktivität** sind standardmäßig ausgeblendet. Die Checkbox „Kunden ohne Aktivität
zeigen" nimmt alle aktiven Kunden in den Bericht, auch die ohne Zeit und ohne Lauf.

**Drucken / PDF:** Der Knopf oben rechts öffnet den Druckdialog. Dort „Als PDF
speichern" wählen – Seitenleiste, Kopfleiste und Knöpfe werden beim Drucken automatisch
ausgeblendet, Kundendaten sind alphabetisch sortiert, jeder Kundenblock bleibt möglichst auf
einer Seite.

**Rechte:** Der Bericht ist lesbar für alle Rollen, auch „Nur Lesen". Damit kann die Kollegin
deines Chefs mit einem eigenen Konto arbeiten, ohne irgendetwas ändern zu können.

Jede Erzeugung landet als **„Monatsbericht erzeugt"** im Audit-Trail, mit Benutzer und Monat.

## Patchstand und Antivirus

NinjaOne liefert beides über zwei Abfragen für den ganzen Bestand
(`/v2/queries/os-patches` und `/v2/queries/antivirus-status`) – kein Rundlauf pro Gerät.
Abgerufen wird beim **NinjaOne**-Knopf oben rechts oder einzeln unter
**Integrationen → Patch- und Antivirusstand → Jetzt abrufen**. Nach 306 Geräten dauert
das rund drei bis vier Sekunden.

**Patchstand**

- **Patches offen** – es gibt genehmigte, zurückgestellte oder manuell freizugebende Patches
- **aktuell** – gescannt, nichts offen; unter der Kachel steht die Zahl der abgelehnten
- **unbekannt** – für dieses Gerät liegt keine Patch-Meldung vor (noch nie gescannt).

In eurem Bestand haben 126 von 306 Geräten Betriebssystem-Patchdaten. Bei den übrigen bleibt
„unbekannt" stehen – das ist ehrlicher als ein grüner Haken ohne Grundlage.

**Betriebssystem und Software getrennt**

NinjaOne führt zwei getrennte Patch-Listen. Beide werden abgerufen; im Geräte-Panel schaltet
der Reiter **Betriebssystem / Software** um. Angezeigt wird zuerst die Liste, zu der es
Einträge gibt.

- **Betriebssystem** – Windows-Updates mit KB-Nummer und Schweregrad
- **Software** – Programme wie 7-Zip, Chrome oder 1Password mit Empfehlungsstufe

Die Software-Liste ist deutlich größer: rund 24.000 Einträge im Test-Bestand, fast alle
abgelehnt. In der Ampel zählen beide zusammen – offen ist offen, egal ob Betriebssystem oder
Programm. Die Aufteilung steht unter **Integrationen** in der Zeile „Patch & Antivirus".

**Abgelehnte Patches** zählen nicht als offene Arbeit, werden aber angezeigt: sie erscheinen
in der Liste grau mit „abgelehnt" und in der Kachel als „N abgelehnt". Ein Gerät, bei dem nur
Abgelehntes steht, gilt als aktuell.

**Antivirus**

Ein Gerät meldet oft mehrere Produkte: Bitdefender aktiv, Windows Defender abgeschaltet.
Das Cockpit zeigt das Produkt, das **wirklich schützt**, also das eingeschaltete. Steht
Bitdefender auf `OFF` und Defender übernimmt, gilt das Gerät als geschützt. Erst wenn kein
Produkt läuft oder die Signaturen veraltet sind, steht **prüfen** in der Kachel.

Wichtig für die Praxis: 136 der 506 gemeldeten Produkteinträge stehen auf `OFF` – das ist
fast immer der abgeschaltete Defender neben Bitdefender und **kein** Alarm. Genau deshalb
wird das aktive Produkt gewählt und nicht einfach jedes `OFF` gezählt.

**Handlungsbedarf in der Übersicht**

Antiviruswarnungen stehen oben, danach Geräte mit offenen Patches (die Zahl der offenen
Patches steht im Text). Die Kundenliste markiert einen Kunden als „Handlungsbedarf", sobald
eines seiner Geräte offene Patches hat.

## Am Gerät ausführen

Im Geräte-Panel unter **Kunden → Gerät anzeigen** steht der Block **„Am Gerät ausführen"** –
genau die vier Patch-Befehle aus dem NinjaOne-Menü, plus Neustart und Wartungsmodus:

| Gruppe | Knopf | was passiert |
| --- | --- | --- |
| Prüfen | **OS-Scan** | lässt Windows-Updates neu prüfen, installiert nichts |
| Prüfen | **Software-Scan** | lässt installierte Programme neu prüfen, installiert nichts |
| Installieren | **OS-Updates** | installiert alle genehmigten Windows-Updates |
| Installieren | **Software-Updates** | installiert alle genehmigten Programm-Updates |
| Gerät | **Neustart** | startet neu, mit Begründung und optional „erzwingen" |
| Gerät | **Wartungsmodus** | unterdrückt Alarme für 15 Minuten bis 4 Stunden |

Scans und der Script-Test laufen sofort. Alles, was am Gerät etwas verändert, fragt vorher
nach – ein Fehlklick soll nicht installieren. Der Knopf ist dann rot.

**Was das Cockpit nicht kann: Patches genehmigen.** Die NinjaOne-API hat dafür keinen
Endpunkt; freigegeben wird ausschließlich über die Patch-Policy in NinjaOne. Das Cockpit zeigt
deshalb an, was offen, genehmigt und abgelehnt ist, und spielt auf Klick ein, was genehmigt
wurde. Für die tägliche Arbeit ist eine Policy-Regel („kritisch nach 3 Tagen automatisch
genehmigen") meist die bessere Lösung als Genehmigen von Hand.

**Wartungsmodus:** NinjaOne hat keinen Lese-Endpunkt für ein gesetztes Wartungsfenster. Das
Cockpit merkt sich deshalb selbst, bis wann es gesetzt wurde, und zeigt das im Panel an. Wird
das Fenster in NinjaOne geändert, weiß das Cockpit davon nichts – „Modus beenden" räumt trotzdem
auf.

**Grenzen und Verhalten**

- Ein Gerät, ein Vorgang: solange ein Auftrag läuft, sind die Knöpfe gesperrt (Antwort 409).
- Offline-Geräte werden abgelehnt (Ausnahme: „Modus beenden"). NinjaOne würde den Auftrag
  annehmen und erst beim nächsten Kontakt ausführen – das ist keine Arbeit, die man gerade macht.
- Nach dem Absenden wartet das Cockpit auf die Abschluss-Meldung in der NinjaOne-Gerätehistorie
  und zeigt sie im Panel an. Meldet NinjaOne einen Fehler (z. B. „nicht genügend Speicherplatz"),
  steht der Lauf als **Fehler** und der Run als **blockiert** – nicht als erledigt.
- Alle Aktionen landen im Audit-Trail mit Akteur und im Lauf **„Sofortaktionen <Monat>"** des
  Kunden. Ein Lauf pro Kunde und Monat, damit die Wartungsliste nicht mit Einzeilern zuläuft.

## Automatischer Timer-Start

Der Timer startet von selbst, sobald du mit der Arbeit anfängst:

- **NinjaOne:** sobald du in einem Kunden ein Gerät über **Anzeigen** öffnest
- **Website:** sobald du in der **Website-Vorschau** auf eine Seite klickst

Beim **Schließen des Geräte-Panels** endet die Arbeit am Gerät: Der Timer wird gestoppt und der
Kommentar-Dialog öffnet sich. Wer das Panel nur kurz zum Nachsehen schließt, verliert so keine
Zeit im Hintergrund.

**Wichtig:** Ein bereits laufender Timer wird beim Öffnen **nicht** beendet. Wer gerade bei einem
anderen Kunden misst, bekommt den Hinweis „Timer läuft noch für …" und die Zeit bleibt, wo sie
ist. Erst nach dem Stoppen startet der nächste Klick den neuen Timer. Sonst würde die
automatische Erfassung stillschweigend Abrechnungszeit abschneiden.

## Laufender Timer in der Kopfleiste

Sobald ein Timer läuft, erscheint oben rechts ein roter Kasten: Kunde, Tätigkeit, die laufende
Zeit im Sekundentakt und daneben **Stop**. Ohne laufenden Timer ist der Kasten ausgeblendet –
die Kopfleiste zeigt dann nur den NinjaOne-Knopf.

Der Timer läuft beim Seitenwechsel weiter; er hängt am Konto, nicht an der Ansicht. Ein zweiter
Timer lässt sich nicht starten, solange einer läuft.

## Dialoge

Dialoge (Einsatz anlegen, Zeit erfassen, Benutzer anlegen …) sind **verschiebbar**: am Kopf
packen und ziehen. Praktisch, wenn dahinter etwas zu lesen ist. Sie lassen sich nicht aus dem
Fenster schieben – ein Teil bleibt immer sichtbar, damit der Kopf zum Zurückziehen erreichbar
bleibt. **Esc** oder das Kreuz schließt.

## Lange Listen

Wo Listen lang werden, scrollt nur der Inhalt, nicht die ganze Seite:

- **Übersicht → Letzte Aktivität:** zeigt drei Einträge, der Rest ist im Kasten scrollbar
- **Kunden** und **Websites:** die Tabelle scrollt in der Karte, die Kopfzeile bleibt stehen
- **Zeiten → Stunden nach Kunde** und **Vertragslogik:** je eigener Scrollbereich
- **Integrationen → Audit-Trail:** acht Zeilen sichtbar, dann scrollen

Der Platz richtet sich nach der Fensterhöhe, nicht nach festen Pixeln – auf einem kleinen
Bildschirm wird also nichts abgeschnitten.

## tix-Ticketsystem

tix hat keine dokumentierte Schnittstelle. Im Cockpit gibt es deshalb nur den Weg dorthin:

- **Einsätze → In tix nachsehen** öffnet das Ticketsystem im neuen Tab
- **Integrationen** hat eine eigene Karte mit der Adresse

Das Cockpit erfasst die Einsätze selbst; tix bleibt das System, in dem die Kunden ihre Anfragen
stellen. Eine echte Anbindung würde eine API von consultingME voraussetzen.

## Was noch fehlt

- Patches genehmigen oder ablehnen (kein Endpunkt in der NinjaOne-API – siehe „Am Gerät ausführen")
- Script-Auswahl aus der NinjaOne-Bibliothek in der Oberfläche
- Virenschutz-Scan anstoßen (die API bietet nur Neustart, Wartung, Dienste und Patches)
- tix-Anbindung (das System hat keine API – deshalb nur der Link)
