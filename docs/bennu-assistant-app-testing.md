# Bennu — l'assistente che prova l'applicativo (proposta, senza integrazione)

Stato: **proposta**. Niente di questo è implementato. Serve a decidere *se* e *fino a dove* farlo.

## Il problema

Oggi l'assistente legge, modifica e (con conferma) compila e lancia i test. Si ferma prima della domanda
che conta di più dopo una modifica: **l'applicazione, avviata, fa quello che doveva?** Un refactoring che
compila e passa i test può rompere un endpoint, una query o una pagina. Provare a mano costa tempo ed è
proprio il lavoro che il tuo target (applicazioni legacy Java con pochi test) lascia all'uomo.

Il valore pieno si vede in un ciclo: *modifico → avvio → provo → leggo log e risposta → correggo*. Per questo la
migrazione guidata (idea 7: Struts → Spring, `javax` → `jakarta`) ha senso **solo** se esiste questo ciclo:
senza, ogni passo si ferma a "compila", che nel legacy dice poco.

## Che cosa significa "provare l'applicativo"

Quattro livelli, dal più economico al più costoso. Ognuno è utile da solo.

| Livello | Cosa fa | Cosa serve | Costo/rischio |
|---|---|---|---|
| **1. Osservare** | Avvia con una *run configuration* esistente, aspetta che sia pronta, legge i log, segnala eccezioni e stack trace. | Run config (c'è), log della run (c'è), un criterio di "pronta". | Basso |
| **2. Chiamare** | Fa richieste HTTP all'applicazione avviata (solo `localhost`) e controlla stato, corpo, header. | Un tool `http_request` con allowlist. | Medio |
| **3. Guardare i dati** | Controlla che una richiesta abbia scritto/letto ciò che doveva, con una query di sola lettura sul DB di sviluppo. | Picus (c'è), connessione marcata "sviluppo". | Medio-alto |
| **4. Usare la UI** | Pilota una pagina web (clic, form, attese) e legge ciò che appare. | Un browser automatizzabile. | Alto |

Il livello 1 e 2 coprono la maggior parte del valore per applicazioni web legacy: gli errori stanno nei log
e nelle risposte, non nei pixel.

## Cosa c'è già in Bennu

- **Run configuration** e avvio (`run_config`, `tomcat`, `run_script`): sanno come si avvia il progetto.
- **Log della run in streaming**, già interpretati (livelli, stack trace, frame cliccabili: `arbor-logscan`).
- **DTO Lab**: valida un payload JSON contro un DTO, utile per costruire richieste corrette.
- **Picus**: connessioni al DB e query in sola lettura.
- **Stack trace → assistente**: il pulsante sulla traccia nel pannello log (già fatto) è il punto in cui un
  fallimento osservato entra nel ciclo di correzione.

Manca: avviare e fermare *come strumento dell'agente*, sapere quando è pronta, fare richieste.

## Come potrebbe essere (senza scriverlo)

Strumenti nuovi per l'agente, in una famiglia **"Esecuzione"** che si accende a parte (vedi sotto):

- `app_start(config)` — avvia con una run configuration, restituisce un id di sessione.
- `app_wait_ready(id, port | pattern_log, timeout)` — attende una porta aperta o una riga di log, poi dice pronto/non pronto e mostra la coda del log.
- `app_logs(id, since, level)` — il log interpretato, con le eccezioni già raggruppate.
- `http_request(id, method, path, headers?, body?)` — **solo** verso la porta dell'applicazione avviata; restituisce stato, header, corpo (troncato).
- `app_stop(id)`.

Il ciclo lo fa il modello, come per ogni altro compito; il lavoro di sistema è dare strumenti **ristretti**.

## Sicurezza: è la parte che decide se si può fare

Avviare un'applicazione e farle fare richieste è eseguire codice del progetto: come la compilazione, ma più a
lungo e con effetti fuori dal processo.

1. **Consenso esplicito per sessione.** Primo uso = una conferma che dice cosa verrà avviato e con quale configurazione, come per le altre azioni distruttive.
2. **Solo `localhost`.** `http_request` non accetta host: la destinazione è quella dell'applicazione che *Bennu* ha avviato (porta letta dalla run config o dal log). Nessun URL libero, quindi niente SSRF né richieste verso sistemi veri.
3. **Mai produzione.** L'applicazione legacy spesso punta a un DB reale. Prima di avviare: leggere l'URL del datasource dalla configurazione e **rifiutare** se non è su una lista di host "sviluppo" scelta dall'utente (default: solo `localhost`/`127.0.0.1`). È il controllo più importante.
4. **Limiti di risorse.** Durata massima per sessione, dimensione massima di log e risposte, un solo processo alla volta, fermata automatica a fine run o a cancellazione.
5. **Segreti.** Le risposte e i log passano da un mascheramento (header `Authorization`, cookie, valori che somigliano a token) prima di arrivare al modello.
6. **Budget.** Il ciclo consuma token: vale il limite in euro già esistente, più un massimo di cicli *avvia–prova–correggi* per richiesta.
7. **Modifiche ai dati.** `http_request` con metodi che scrivono (POST/PUT/DELETE) chiede conferma la prima volta per percorso, perché su un DB di sviluppo "scrivere" non è innocuo.

## Livello 4 (UI): perché non lo propongo adesso

Pilotare un browser richiede o una libreria di automazione (Playwright/CDP — **nuova dipendenza, serve il tuo ok**) o un
WebView controllato. È il livello più fragile (selettori, attese, stato) e il meno necessario per il legacy
server-rendered, dove le risposte HTTP bastano. Lo terrei fuori finché 1–2 non sono stabili.

## Fasi suggerite

1. **Osservare** (livello 1): `app_start`/`app_wait_ready`/`app_logs`/`app_stop`, consenso, controllo datasource. Già utile da solo: "avvia e dimmi se parte pulito".
2. **Chiamare** (livello 2): `http_request` con le regole sopra.
3. **Migrazione guidata** che usa 1+2 come verifica a ogni passo.
4. (Poi, se serve) dati (livello 3) e UI (livello 4).

## Domande aperte per te

- Le tue applicazioni partono con una **run configuration** già pronta in Bennu, o l'avvio è fatto fuori (script, server esterno)? Cambia molto il livello 1.
- C'è un **criterio di "pronta"** affidabile (una riga di log, una porta, un endpoint di health)?
- I DB che usano in sviluppo sono **separati** da quelli veri, e riconoscibili dall'URL?
- Ti basta il livello 2 (HTTP) o vuoi la UI?
- Le sessioni di prova devono poter **scrivere** sul DB di sviluppo, o solo leggere?

## Stima

Livello 1: contenuto, perché avvio, log e run config esistono. Livello 2: piccolo codice ma molto lavoro di
rifinitura sulle regole. La parte costosa non è il codice: è decidere i confini (consenso, datasource, mascheramento) e
provarli su un progetto vero.
