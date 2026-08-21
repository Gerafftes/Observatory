// Internationalization - DE/EN/PL language support
// Detects browser language, persists choice, translates UI strings

const translations = {
  en: {
    // Navigation
    'nav.dashboard': 'Dashboard',
    'nav.hardware': 'Hardware',
    'nav.demo': 'Live Demo',
    'nav.architecture': 'Architecture',
    'nav.performance': 'Performance',
    'nav.applications': 'Applications',
    'nav.sensing': 'Sensing',
    'nav.training': 'Training',
    'nav.more': 'More',
    'nav.moreExplore': 'Explore',
    'nav.moreTools': 'Tools',
    'nav.poseFusion': 'Pose Fusion',
    'nav.observatory': 'Observatory',

    // Dashboard
    'dashboard.title': 'Revolutionary WiFi-Based Human Pose Detection',
    'dashboard.subtitle': 'Human Tracking Through Walls Using WiFi Signals',
    'dashboard.description': 'AI can track your full-body movement through walls using just WiFi signals. Researchers at Carnegie Mellon have trained a neural network to turn basic WiFi signals into detailed wireframe models of human bodies.',
    'dashboard.status': 'System Status',
    'dashboard.metrics': 'System Metrics',
    'dashboard.features': 'Features',
    'dashboard.liveStats': 'Live Statistics',
    'dashboard.activePersons': 'Active Persons',
    'dashboard.avgConfidence': 'Avg Confidence',
    'dashboard.totalDetections': 'Total Detections',
    'dashboard.zoneOccupancy': 'Zone Occupancy',

    // Status
    'status.apiServer': 'API Server',
    'status.hardware': 'Hardware',
    'status.inference': 'Inference',
    'status.streaming': 'Streaming',
    'status.dataSource': 'Data Source',

    // Metrics
    'metrics.cpu': 'CPU Usage',
    'metrics.memory': 'Memory Usage',
    'metrics.disk': 'Disk Usage',

    // Benefits
    'benefit.throughWalls': 'Through Walls',
    'benefit.throughWallsDesc': 'Works through solid barriers with no line of sight required',
    'benefit.privacy': 'Privacy-Preserving',
    'benefit.privacyDesc': 'No cameras or visual recording - just WiFi signal analysis',
    'benefit.realtime': 'Real-Time',
    'benefit.realtimeDesc': 'Maps 24 body regions in real-time at 100Hz sampling rate',
    'benefit.lowCost': 'Low Cost',
    'benefit.lowCostDesc': 'Built using $30 commercial WiFi hardware',

    // Stats
    'stat.bodyRegions': 'Body Regions',
    'stat.samplingRate': 'Sampling Rate',
    'stat.accuracy': 'Accuracy (AP@50)',
    'stat.hardwareCost': 'Hardware Cost',

    // Actions
    'action.startDetection': 'Start Detection',
    'action.stopDetection': 'Stop Detection',
    'action.toggleTheme': 'Toggle theme',
    'action.exportData': 'Export data',
    'action.screenshot': 'Take screenshot',

    // Connection
    'conn.connected': 'Connected',
    'conn.connecting': 'Connecting...',
    'conn.offline': 'Offline',
    'conn.reconnecting': 'Reconnecting...',
    'conn.live': 'Live',
    'conn.simulated': 'Simulated',

    // Misc
    'misc.loading': 'Loading...',
    'misc.error': 'An error occurred',
    'misc.noData': 'No data available',
    'misc.close': 'Close',
    'misc.cancel': 'Cancel',
    'misc.confirm': 'Confirm',
    'misc.settings': 'Settings',
    'misc.language': 'Language',

    // Quick settings
    'settings.title': 'Settings',
    'settings.display': 'Display',
    'settings.reducedMotion': 'Reduced motion',
    'settings.highContrast': 'High contrast',
    'settings.compactMode': 'Compact mode',
    'settings.monitoring': 'Monitoring',
    'settings.healthPolling': 'Health polling',
    'settings.autoReconnect': 'Auto-reconnect',
    'settings.data': 'Data',
    'settings.clearLocalData': 'Clear local data',
    'settings.resetOnboarding': 'Reset onboarding',
    'settings.clear': 'Clear',
    'settings.reset': 'Reset'
  },

  de: {
    // Navigation
    'nav.dashboard': 'Dashboard',
    'nav.hardware': 'Hardware',
    'nav.demo': 'Live-Demo',
    'nav.architecture': 'Architektur',
    'nav.performance': 'Leistung',
    'nav.applications': 'Anwendungen',
    'nav.sensing': 'Sensing',
    'nav.training': 'Training',
    'nav.more': 'Mehr',
    'nav.moreExplore': 'Übersicht',
    'nav.moreTools': 'Werkzeuge',
    'nav.poseFusion': 'Pose Fusion',
    'nav.observatory': 'Observatory',

    // Dashboard
    'dashboard.title': 'Körperpose durch Wände mit WiFi erkennen',
    'dashboard.subtitle': 'Menschen durch Wände mit WiFi-Signalen verfolgen',
    'dashboard.description': 'WiFi-Signale werden in Informationen über Bewegung und Körperpose übersetzt.',
    'dashboard.status': 'Systemstatus',
    'dashboard.metrics': 'Systemmetriken',
    'dashboard.features': 'Funktionen',
    'dashboard.liveStats': 'Live-Statistiken',
    'dashboard.activePersons': 'Aktive Personen',
    'dashboard.avgConfidence': 'Durchschnittliche Konfidenz',
    'dashboard.totalDetections': 'Detektionen gesamt',
    'dashboard.zoneOccupancy': 'Zonenbelegung',

    // Status
    'status.apiServer': 'API-Server',
    'status.hardware': 'Hardware',
    'status.inference': 'Inferenz',
    'status.streaming': 'Streaming',
    'status.dataSource': 'Datenquelle',

    // Metrics
    'metrics.cpu': 'CPU-Auslastung',
    'metrics.memory': 'Speicherauslastung',
    'metrics.disk': 'Festplattenauslastung',

    // Benefits
    'benefit.throughWalls': 'Durch Wände',
    'benefit.throughWallsDesc': 'Funktioniert ohne direkte Sichtverbindung durch feste Barrieren.',
    'benefit.privacy': 'Privatsphäre',
    'benefit.privacyDesc': 'Keine Kameras oder Videoaufnahmen — nur WiFi-Signalanalyse.',
    'benefit.realtime': 'Echtzeit',
    'benefit.realtimeDesc': 'Erfasst Körperregionen während des laufenden Streams.',
    'benefit.lowCost': 'Geringe Kosten',
    'benefit.lowCostDesc': 'Auf Basis handelsüblicher WiFi-Hardware.',

    // Stats
    'stat.bodyRegions': 'Körperregionen',
    'stat.samplingRate': 'Abtastrate',
    'stat.accuracy': 'Genauigkeit (AP@50)',
    'stat.hardwareCost': 'Hardwarekosten',

    // Actions
    'action.startDetection': 'Detektion starten',
    'action.stopDetection': 'Detektion stoppen',
    'action.toggleTheme': 'Darstellung wechseln',
    'action.exportData': 'Daten exportieren',
    'action.screenshot': 'Screenshot aufnehmen',

    // Connection
    'conn.connected': 'Verbunden',
    'conn.connecting': 'Verbinde …',
    'conn.offline': 'Offline',
    'conn.reconnecting': 'Verbinde erneut …',
    'conn.live': 'Live',
    'conn.simulated': 'Simuliert',

    // Misc
    'misc.loading': 'Lade …',
    'misc.error': 'Ein Fehler ist aufgetreten',
    'misc.noData': 'Keine Daten verfügbar',
    'misc.close': 'Schließen',
    'misc.cancel': 'Abbrechen',
    'misc.confirm': 'Bestätigen',
    'misc.settings': 'Einstellungen',
    'misc.language': 'Sprache',

    // Quick settings
    'settings.title': 'Einstellungen',
    'settings.display': 'Darstellung',
    'settings.reducedMotion': 'Weniger Bewegung',
    'settings.highContrast': 'Hoher Kontrast',
    'settings.compactMode': 'Kompakte Ansicht',
    'settings.monitoring': 'Überwachung',
    'settings.healthPolling': 'Gesundheitsabfragen',
    'settings.autoReconnect': 'Automatisch verbinden',
    'settings.data': 'Daten',
    'settings.clearLocalData': 'Lokale Daten löschen',
    'settings.resetOnboarding': 'Einführung zurücksetzen',
    'settings.clear': 'Löschen',
    'settings.reset': 'Zurücksetzen'
  },

  pl: {
    // Navigation
    'nav.dashboard': 'Panel',
    'nav.hardware': 'Sprzet',
    'nav.demo': 'Demo na zywo',
    'nav.architecture': 'Architektura',
    'nav.performance': 'Wydajnosc',
    'nav.applications': 'Aplikacje',
    'nav.sensing': 'Czujniki',
    'nav.training': 'Trening',
    'nav.more': 'Wiecej',
    'nav.moreExplore': 'Przegladaj',
    'nav.moreTools': 'Narzedzia',
    'nav.poseFusion': 'Pose Fusion',
    'nav.observatory': 'Observatory',

    // Dashboard
    'dashboard.title': 'Rewolucyjne wykrywanie pozy czlowieka przez WiFi',
    'dashboard.subtitle': 'Sledzenie ludzi przez sciany za pomoca sygnalow WiFi',
    'dashboard.description': 'AI moze sledzic ruchy calego ciala przez sciany uzywajac jedynie sygnalow WiFi. Badacze z Carnegie Mellon wytrenowali siec neuronowa do zamiany sygnalow WiFi w szczegolowe modele szkieletowe.',
    'dashboard.status': 'Status systemu',
    'dashboard.metrics': 'Metryki systemu',
    'dashboard.features': 'Funkcje',
    'dashboard.liveStats': 'Statystyki na zywo',
    'dashboard.activePersons': 'Aktywne osoby',
    'dashboard.avgConfidence': 'Srednia pewnosc',
    'dashboard.totalDetections': 'Laczne detekcje',
    'dashboard.zoneOccupancy': 'Zajecie stref',

    // Status
    'status.apiServer': 'Serwer API',
    'status.hardware': 'Sprzet',
    'status.inference': 'Wnioskowanie',
    'status.streaming': 'Streaming',
    'status.dataSource': 'Zrodlo danych',

    // Metrics
    'metrics.cpu': 'Uzycie CPU',
    'metrics.memory': 'Uzycie pamieci',
    'metrics.disk': 'Uzycie dysku',

    // Benefits
    'benefit.throughWalls': 'Przez sciany',
    'benefit.throughWallsDesc': 'Dziala przez przeszkody stale bez linii wzroku',
    'benefit.privacy': 'Ochrona prywatnosci',
    'benefit.privacyDesc': 'Brak kamer i nagrywania - tylko analiza sygnalow WiFi',
    'benefit.realtime': 'Czas rzeczywisty',
    'benefit.realtimeDesc': 'Mapuje 24 regiony ciala w czasie rzeczywistym przy 100Hz',
    'benefit.lowCost': 'Niski koszt',
    'benefit.lowCostDesc': 'Zbudowany z komercyjnego sprzetu WiFi za $30',

    // Stats
    'stat.bodyRegions': 'Regiony ciala',
    'stat.samplingRate': 'Czestotliwosc',
    'stat.accuracy': 'Dokladnosc (AP@50)',
    'stat.hardwareCost': 'Koszt sprzetu',

    // Actions
    'action.startDetection': 'Rozpocznij detekcje',
    'action.stopDetection': 'Zatrzymaj detekcje',
    'action.toggleTheme': 'Zmien motyw',
    'action.exportData': 'Eksportuj dane',
    'action.screenshot': 'Zrob zrzut ekranu',

    // Connection
    'conn.connected': 'Polaczono',
    'conn.connecting': 'Laczenie...',
    'conn.offline': 'Offline',
    'conn.reconnecting': 'Ponowne laczenie...',
    'conn.live': 'Na zywo',
    'conn.simulated': 'Symulacja',

    // Misc
    'misc.loading': 'Ladowanie...',
    'misc.error': 'Wystapil blad',
    'misc.noData': 'Brak danych',
    'misc.close': 'Zamknij',
    'misc.cancel': 'Anuluj',
    'misc.confirm': 'Potwierdz',
    'misc.settings': 'Ustawienia',
    'misc.language': 'Jezyk',

    // Quick settings
    'settings.title': 'Ustawienia',
    'settings.display': 'Widok',
    'settings.reducedMotion': 'Mniej animacji',
    'settings.highContrast': 'Wysoki kontrast',
    'settings.compactMode': 'Tryb kompaktowy',
    'settings.monitoring': 'Monitoring',
    'settings.healthPolling': 'Odpytywanie stanu',
    'settings.autoReconnect': 'Automatyczne laczenie',
    'settings.data': 'Dane',
    'settings.clearLocalData': 'Wyczysc dane lokalne',
    'settings.resetOnboarding': 'Resetuj wprowadzenie',
    'settings.clear': 'Wyczysc',
    'settings.reset': 'Resetuj'
  }
};

export class I18n {
  constructor() {
    this.locale = this.getSavedLocale() || this.detectLocale();
    this.listeners = [];
  }

  init() {
    document.documentElement.setAttribute('lang', this.locale);
    this.createSelector();
    this.applyTranslations();
  }

  detectLocale() {
    const lang = navigator.language?.toLowerCase() || 'en';
    if (lang.startsWith('de')) return 'de';
    if (lang.startsWith('pl')) return 'pl';
    return 'en';
  }

  getSavedLocale() {
    try { return localStorage.getItem('ruview-locale'); }
    catch { return null; }
  }

  saveLocale(locale) {
    try { localStorage.setItem('ruview-locale', locale); }
    catch { /* noop */ }
  }

  t(key) {
    const dict = translations[this.locale] || translations.en;
    return dict[key] || translations.en[key] || key;
  }

  setLocale(locale) {
    if (!translations[locale]) return;
    this.locale = locale;
    this.saveLocale(locale);
    document.documentElement.setAttribute('lang', locale);
    this.applyTranslations();
    this.listeners.forEach(cb => { try { cb(locale); } catch { /* noop */ } });
  }

  onLocaleChange(callback) {
    this.listeners.push(callback);
    return () => {
      const i = this.listeners.indexOf(callback);
      if (i > -1) this.listeners.splice(i, 1);
    };
  }

  applyTranslations() {
    // Translate elements with data-i18n attribute
    document.querySelectorAll('[data-i18n]').forEach(el => {
      const key = el.getAttribute('data-i18n');
      el.textContent = this.t(key);
    });

    // Translate placeholders
    document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
      const key = el.getAttribute('data-i18n-placeholder');
      el.placeholder = this.t(key);
    });

    // Translate aria-labels
    document.querySelectorAll('[data-i18n-aria]').forEach(el => {
      const key = el.getAttribute('data-i18n-aria');
      el.setAttribute('aria-label', this.t(key));
    });

    // Translate titles on compact controls
    document.querySelectorAll('[data-i18n-title]').forEach(el => {
      const key = el.getAttribute('data-i18n-title');
      el.setAttribute('title', this.t(key));
    });

    // Update language selector
    const selector = document.getElementById('lang-selector');
    if (selector) selector.value = this.locale;
  }

  createSelector() {
    const wrapper = document.createElement('div');
    wrapper.className = 'lang-selector-wrap';
    wrapper.innerHTML = `
      <span class="lang-selector-label" data-i18n="misc.language">Language</span>
      <select id="lang-selector" class="lang-selector" aria-label="Language" data-i18n-aria="misc.language">
        <option value="de">DE</option>
        <option value="en">EN</option>
        <option value="pl">PL</option>
      </select>
    `;

    const select = wrapper.querySelector('select');
    select.value = this.locale;
    select.addEventListener('change', () => this.setLocale(select.value));

    const headerActions = document.querySelector('[data-header-actions]') || document.querySelector('.header-info');
    if (headerActions) {
      headerActions.appendChild(wrapper);
    }
  }

  getAvailableLocales() {
    return Object.keys(translations);
  }

  dispose() {
    this.listeners = [];
  }
}

export const i18n = new I18n();
