//! Traduções. As chaves são os textos em português; cada linha da tabela é [pt, en, es, it, fr].
//! `tr` devolve o texto no idioma ativo (ou a própria chave se não houver tradução).
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang { Pt = 0, En = 1, Es = 2, It = 3, Fr = 4 }

static LANG: AtomicU8 = AtomicU8::new(0);

const ALL: [Lang; 5] = [Lang::Pt, Lang::En, Lang::Es, Lang::It, Lang::Fr];
/// Valores gravados na base de dados (a ordem é a do seletor, depois de "auto").
pub const CODES: [&str; 5] = ["pt", "en", "es", "it", "fr"];
pub const NAMES: [&str; 5] = ["Português", "English", "Español", "Italiano", "Français"];

pub fn set(l: Lang) { LANG.store(l as u8, Ordering::Relaxed); }
pub fn get() -> Lang { ALL[(LANG.load(Ordering::Relaxed) as usize).min(4)] }

/// Idioma da interface do Windows; qualquer outro → inglês.
pub fn system_lang() -> Lang {
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    from_langid(id)
}

fn from_langid(id: u16) -> Lang {
    match id & 0x3FF { 0x16 => Lang::Pt, 0x0A => Lang::Es, 0x10 => Lang::It, 0x0C => Lang::Fr, _ => Lang::En }
}

/// "auto" (ou valor desconhecido) segue o sistema.
pub fn resolve(setting: &str) -> Lang {
    CODES.iter().position(|c| *c == setting).map(|i| ALL[i]).unwrap_or_else(system_lang)
}

/// Índice do seletor de idioma (0 = automático).
pub fn setting_index(setting: &str) -> usize { CODES.iter().position(|c| *c == setting).map(|i| i + 1).unwrap_or(0) }
pub fn setting_of_index(i: usize) -> &'static str { if i == 0 { "auto" } else { CODES[(i - 1).min(4)] } }

pub fn has(key: &str) -> bool { T.iter().any(|r| r[0] == key) }

pub fn tr(key: &'static str) -> &'static str {
    let l = get() as usize;
    if l == 0 { return key; }
    T.iter().find(|r| r[0] == key).map(|r| r[l]).unwrap_or(key)
}

/// Substitui os `{}` por ordem.
pub fn fill(s: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let mut it = args.iter();
    let mut rest = s;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = it.next() { out.push_str(a); }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// Separador decimal do idioma ativo.
pub fn dec() -> char { if get() == Lang::En { '.' } else { ',' } }

const T: &[[&str; 5]] = &[
    // ---- eventos ----
    ["Falha de energia", "Power failure", "Corte de energía", "Interruzione di corrente", "Panne de courant"],
    ["Subtensão", "Undervoltage", "Subtensión", "Sottotensione", "Sous-tension"],
    ["Sobretensão", "Overvoltage", "Sobretensión", "Sovratensione", "Surtension"],
    ["Regulação Boost", "Boost regulation", "Regulación Boost", "Regolazione Boost", "Régulation Boost"],
    ["Regulação Buck", "Buck regulation", "Regulación Buck", "Regolazione Buck", "Régulation Buck"],
    ["Fim de regulação", "End of regulation", "Fin de la regulación", "Fine della regolazione", "Fin de la régulation"],
    ["Bateria fraca", "Low battery", "Batería baja", "Batteria scarica", "Batterie faible"],
    ["Comunicação com a UPS perdida", "UPS communication lost", "Comunicación con la UPS perdida", "Comunicazione con l'UPS persa", "Communication avec l'onduleur perdue"],
    ["Comunicação com a UPS restabelecida", "UPS communication restored", "Comunicación con la UPS restablecida", "Comunicazione con l'UPS ripristinata", "Communication avec l'onduleur rétablie"],
    ["Energia restabelecida", "Power restored", "Energía restablecida", "Alimentazione ripristinata", "Courant rétabli"],
    ["Auto-teste iniciado", "Self-test started", "Autotest iniciado", "Autotest avviato", "Autotest démarré"],
    ["Auto-teste concluído: passou", "Self-test completed: passed", "Autotest completado: superado", "Autotest completato: superato", "Autotest terminé : réussi"],
    ["Auto-teste concluído: falhou", "Self-test completed: failed", "Autotest completado: fallido", "Autotest completato: non superato", "Autotest terminé : échoué"],
    ["Desligar o computador iniciado", "Computer shutdown started", "Apagado del equipo iniciado", "Spegnimento del computer avviato", "Arrêt de l'ordinateur démarré"],
    ["Desligar o computador cancelado", "Computer shutdown canceled", "Apagado del equipo cancelado", "Spegnimento del computer annullato", "Arrêt de l'ordinateur annulé"],
    ["Falha ao desligar o computador", "Failed to shut down the computer", "Error al apagar el equipo", "Impossibile spegnere il computer", "Échec de l'arrêt de l'ordinateur"],
    ["Evento", "Event", "Evento", "Evento", "Événement"],
    // ---- estado ----
    ["Fonte de energia", "Power source", "Fuente de energía", "Fonte di alimentazione", "Source d'alimentation"],
    ["Capacidade da bateria", "Battery capacity", "Capacidad de la batería", "Capacità della batteria", "Capacité de la batterie"],
    ["Autonomia estimada", "Estimated runtime", "Autonomía estimada", "Autonomia stimata", "Autonomie estimée"],
    ["Estado atual", "Current status", "Estado actual", "Stato attuale", "État actuel"],
    ["Marca e modelo", "Brand and model", "Marca y modelo", "Marca e modello", "Marque et modèle"],
    ["Energia fornecida por", "Powered by", "Alimentado por", "Alimentato da", "Alimenté par"],
    ["Tensão de saída", "Output voltage", "Tensión de salida", "Tensione di uscita", "Tension de sortie"],
    ["Condição da energia de entrada", "Input power condition", "Estado de la entrada", "Stato dell'ingresso", "État de l'entrée"],
    ["Condição da energia de saída", "Output power condition", "Estado de la salida", "Stato dell'uscita", "État de la sortie"],
    ["Capacidade restante da bateria", "Remaining battery capacity", "Capacidad restante de la batería", "Capacità residua della batteria", "Capacité restante de la batterie"],
    ["Estado da bateria", "Battery status", "Estado de la batería", "Stato della batteria", "État de la batterie"],
    ["Autonomia restante", "Remaining runtime", "Autonomía restante", "Autonomia residua", "Autonomie restante"],
    ["Carga da UPS", "UPS load", "Carga de la UPS", "Carico dell'UPS", "Charge de l'onduleur"],
    ["Estado do hardware", "Hardware status", "Estado del hardware", "Stato dell'hardware", "État du matériel"],
    ["Sem comunicação com a UPS", "No communication with the UPS", "Sin comunicación con la UPS", "Nessuna comunicazione con l'UPS", "Pas de communication avec l'onduleur"],
    ["Bateria", "Battery", "Batería", "Batteria", "Batterie"],
    ["Rede elétrica", "Mains power", "Red eléctrica", "Rete elettrica", "Secteur"],
    ["Normal", "Normal", "Normal", "Normale", "Normal"],
    ["Totalmente carregada", "Fully charged", "Totalmente cargada", "Completamente carica", "Entièrement chargée"],
    ["A carregar", "Charging", "Cargando", "In carica", "En charge"],
    ["A descarregar", "Discharging", "Descargando", "In scarica", "En décharge"],
    ["Sobrecarga", "Overload", "Sobrecarga", "Sovraccarico", "Surcharge"],
    ["{} volts", "{} volts", "{} voltios", "{} volt", "{} volts"],
    ["{} minutos", "{} minutes", "{} minutos", "{} minuti", "{} minutes"],
    // ---- eventos (página) ----
    ["Resumo de problemas de energia", "Power problems summary", "Resumen de problemas eléctricos", "Riepilogo problemi elettrici", "Résumé des incidents électriques"],
    ["Período", "Period", "Período", "Periodo", "Période"],
    ["Última semana", "Last week", "Última semana", "Ultima settimana", "Dernière semaine"],
    ["Último mês", "Last month", "Último mes", "Ultimo mese", "Dernier mois"],
    ["Último ano", "Last year", "Último año", "Ultimo anno", "Dernière année"],
    ["Problema de energia", "Power problem", "Problema de energía", "Problema di alimentazione", "Problème d'alimentation"],
    ["Número de vezes", "Number of times", "Número de veces", "Numero di volte", "Nombre de fois"],
    ["Duração", "Duration", "Duración", "Durata", "Durée"],
    ["Registo de eventos", "Event log", "Registro de eventos", "Registro eventi", "Journal des événements"],
    ["De", "From", "Desde", "Dal", "Du"],
    ["Até", "To", "Hasta", "Al", "Au"],
    ["Todos", "All", "Todos", "Tutti", "Tous"],
    ["Filtrar…", "Filter…", "Filtrar…", "Filtra…", "Filtrer…"],
    ["Procurar", "Search", "Buscar", "Cerca", "Chercher"],
    ["Hora", "Time", "Hora", "Ora", "Heure"],
    ["por página", "per page", "por página", "per pagina", "par page"],
    ["Apagar eventos do período", "Delete events in period", "Borrar eventos del período", "Elimina eventi del periodo", "Supprimer les événements de la période"],
    ["Total em bateria", "Total on battery", "Total con batería", "Totale a batteria", "Total sur batterie"],
    ["Total de regulação", "Total regulation", "Total de regulación", "Totale regolazione", "Total de régulation"],
    ["Nunca", "Never", "Nunca", "Mai", "Jamais"],
    ["Nenhum", "None", "Ninguno", "Nessuna", "Aucune"],
    // ---- energia ----
    ["Relatório de energia", "Energy report", "Informe de energía", "Report energetico", "Rapport d'énergie"],
    ["Atualizar", "Refresh", "Actualizar", "Aggiorna", "Actualiser"],
    ["Consumo médio diário", "Average daily consumption", "Consumo medio diario", "Consumo medio giornaliero", "Consommation moyenne par jour"],
    ["Consumo acumulado", "Total consumption", "Consumo acumulado", "Consumo totale", "Consommation cumulée"],
    ["Custo acumulado", "Total cost", "Coste acumulado", "Costo totale", "Coût cumulé"],
    ["CO2 emitido", "CO2 emitted", "CO2 emitido", "CO2 emessa", "CO2 émis"],
    ["Definições de energia", "Energy settings", "Ajustes de energía", "Impostazioni energia", "Paramètres d'énergie"],
    ["Região", "Region", "Región", "Regione", "Région"],
    ["Custo por kWh", "Cost per kWh", "Coste por kWh", "Costo per kWh", "Coût par kWh"],
    ["CO2 emitido por kWh", "CO2 emitted per kWh", "CO2 emitido por kWh", "CO2 emessa per kWh", "CO2 émis par kWh"],
    ["Unidade de medida", "Unit", "Unidad de medida", "Unità di misura", "Unité de mesure"],
    ["Quilogramas (kg)", "Kilograms (kg)", "Kilogramos (kg)", "Chilogrammi (kg)", "Kilogrammes (kg)"],
    ["Libras (lb)", "Pounds (lb)", "Libras (lb)", "Libbre (lb)", "Livres (lb)"],
    ["Aplicar", "Apply", "Aplicar", "Applica", "Appliquer"],
    ["Valores inválidos", "Invalid values", "Valores no válidos", "Valori non validi", "Valeurs invalides"],
    ["Aplicado com sucesso", "Applied successfully", "Aplicado correctamente", "Applicato con successo", "Appliqué avec succès"],
    // ---- configuração ----
    ["Notificações", "Notifications", "Notificaciones", "Notifiche", "Notifications"],
    ["Sons de software", "Software sounds", "Sonidos del software", "Suoni del software", "Sons du logiciel"],
    ["Ativar sons de notificação", "Enable notification sounds", "Activar sonidos de notificación", "Attiva i suoni di notifica", "Activer les sons de notification"],
    ["Alarmes da UPS", "UPS alarms", "Alarmas de la UPS", "Allarmi dell'UPS", "Alarmes de l'onduleur"],
    ["Ativar alarmes sempre", "Always enable alarms", "Activar siempre las alarmas", "Attiva sempre gli allarmi", "Toujours activer les alarmes"],
    ["Desativar alarmes sempre", "Always disable alarms", "Desactivar siempre las alarmas", "Disattiva sempre gli allarmi", "Toujours désactiver les alarmes"],
    ["Configuração de autonomia", "Runtime settings", "Configuración de autonomía", "Impostazioni di autonomia", "Paramètres d'autonomie"],
    ["Manter o computador a funcionar — desligar quando a autonomia restante for",
     "Keep the computer running — shut down when the remaining runtime is",
     "Mantener el equipo en marcha — apagar cuando la autonomía restante sea de",
     "Mantieni il computer acceso — spegni quando l'autonomia residua è di",
     "Garder l'ordinateur en marche — arrêter quand l'autonomie restante est de"],
    ["Preservar a bateria — desligar o computador após estar em bateria durante",
     "Preserve the battery — shut down the computer after running on battery for",
     "Preservar la batería — apagar el equipo tras estar con batería durante",
     "Preserva la batteria — spegni il computer dopo aver funzionato a batteria per",
     "Préserver la batterie — arrêter l'ordinateur après un fonctionnement sur batterie de"],
    ["minutos", "minutes", "minutos", "minuti", "minutes"],
    ["Idioma", "Language", "Idioma", "Lingua", "Langue"],
    ["Idioma da aplicação", "Application language", "Idioma de la aplicación", "Lingua dell'applicazione", "Langue de l'application"],
    ["Automático (idioma do sistema)", "Automatic (system language)", "Automático (idioma del sistema)", "Automatico (lingua del sistema)", "Automatique (langue du système)"],
    ["Falha ao alterar os alarmes da UPS", "Failed to change the UPS alarms", "No se pudieron cambiar las alarmas de la UPS", "Impossibile modificare gli allarmi dell'UPS", "Échec de la modification des alarmes de l'onduleur"],
    ["A aplicar…", "Applying…", "Aplicando…", "Applicazione…", "Application…"],
    ["Falha ao aplicar", "Failed to apply", "Error al aplicar", "Applicazione non riuscita", "Échec de l'application"],
    ["Sem comunicação", "No communication", "Sin comunicación", "Nessuna comunicazione", "Pas de communication"],
    // ---- UPS ----
    ["Tensão", "Voltage", "Tensión", "Tensione", "Tension"],
    ["Quando a tensão da rede desce abaixo de", "When mains voltage drops below", "Cuando la tensión de red baja de", "Quando la tensione di rete scende sotto", "Lorsque la tension secteur descend sous"],
    ["Quando a tensão da rede sobe acima de", "When mains voltage rises above", "Cuando la tensión de red sube de", "Quando la tensione di rete sale sopra", "Lorsque la tension secteur dépasse"],
    ["volts", "volts", "voltios", "volt", "volts"],
    ["Auto-teste", "Self-test", "Autotest", "Autotest", "Autotest"],
    ["Data do último auto-teste", "Last self-test date", "Fecha del último autotest", "Data dell'ultimo autotest", "Date du dernier autotest"],
    ["Resultado do último auto-teste", "Last self-test result", "Resultado del último autotest", "Risultato dell'ultimo autotest", "Résultat du dernier autotest"],
    ["Iniciar auto-teste", "Start self-test", "Iniciar autotest", "Avvia autotest", "Lancer l'autotest"],
    ["Não suportado por esta UPS", "Not supported by this UPS", "No compatible con esta UPS", "Non supportato da questo UPS", "Non pris en charge"],
    ["A tensão atual é de {} volts.\nA UPS intervém quando a tensão da rede desce abaixo de {} volts ou sobe acima de {} volts.",
     "Current voltage: {} V.\nThe UPS switches to battery below {} V or above {} V.",
     "Tensión actual: {} V.\nLa UPS pasa a batería por debajo de {} V o por encima de {} V.",
     "Tensione attuale: {} V.\nL'UPS passa a batteria sotto i {} V o sopra i {} V.",
     "Tension actuelle : {} V.\nL'onduleur passe sur batterie sous {} V ou au-dessus de {} V."],
    ["Sem comunicação com a UPS.", "No communication with the UPS.", "Sin comunicación con la UPS.", "Nessuna comunicazione con l'UPS.", "Pas de communication avec l'onduleur."],
    ["Em curso…", "In progress…", "En curso…", "In corso…", "En cours…"],
    ["Passou", "Passed", "Superado", "Superato", "Réussi"],
    ["Falhou", "Failed", "Fallido", "Non superato", "Échoué"],
    ["Apagar permanentemente os eventos do período selecionado?", "Permanently delete the events in the selected period?",
     "¿Borrar permanentemente los eventos del período seleccionado?", "Eliminare definitivamente gli eventi del periodo selezionato?",
     "Supprimer définitivement les événements de la période sélectionnée ?"],
    ["Auto-teste iniciado. A UPS vai funcionar a bateria durante o teste.", "Self-test started. The UPS will run on battery during the test.",
     "Autotest iniciado. La UPS funcionará con batería durante la prueba.", "Autotest avviato. L'UPS funzionerà a batteria durante il test.",
     "Autotest démarré. L'onduleur fonctionnera sur batterie pendant le test."],
    ["Não foi possível iniciar o auto-teste.", "Could not start the self-test.", "No se pudo iniciar el autotest.", "Impossibile avviare l'autotest.", "Impossible de lancer l'autotest."],
    // ---- barra lateral, bandeja, avisos ----
    ["Estado", "Status", "Estado", "Stato", "État"],
    ["Eventos", "Events", "Eventos", "Eventi", "Événements"],
    ["Energia", "Energy", "Energía", "Energia", "Énergie"],
    ["Configuração", "Settings", "Configuración", "Impostazioni", "Paramètres"],
    ["Em bateria", "On battery", "Con batería", "A batteria", "Sur batterie"],
    ["UPS não encontrada", "UPS not found", "UPS no encontrada", "UPS non trovato", "Onduleur introuvable"],
    ["AC", "AC", "CA", "CA", "CA"],
    ["UPS: sem comunicação", "UPS: no communication", "UPS: sin comunicación", "UPS: nessuna comunicazione", "Onduleur : pas de communication"],
    ["Abrir", "Open", "Abrir", "Apri", "Ouvrir"],
    ["Auto-teste da UPS", "UPS self-test", "Autotest de la UPS", "Autotest dell'UPS", "Autotest de l'onduleur"],
    ["Sair", "Exit", "Salir", "Esci", "Quitter"],
    ["Falha de energia: a UPS passou para bateria", "Power failure: the UPS switched to battery", "Corte de energía: la UPS ha pasado a batería", "Interruzione di corrente: l'UPS è passato a batteria", "Panne de courant : l'onduleur est passé sur batterie"],
    ["Subtensão: a UPS passou para bateria", "Undervoltage: the UPS switched to battery", "Subtensión: la UPS ha pasado a batería", "Sottotensione: l'UPS è passato a batteria", "Sous-tension : l'onduleur est passé sur batterie"],
    ["Sobretensão: a UPS passou para bateria", "Overvoltage: the UPS switched to battery", "Sobretensión: la UPS ha pasado a batería", "Sovratensione: l'UPS è passato a batteria", "Surtension : l'onduleur est passé sur batterie"],
    ["O computador vai desligar em 30 s", "The computer will shut down in 30 s", "El equipo se apagará en 30 s", "Il computer si spegnerà tra 30 s", "L'ordinateur va s'arrêter dans 30 s"],
    ["Desligar cancelado", "Shutdown canceled", "Apagado cancelado", "Spegnimento annullato", "Arrêt annulé"],
    ["Lite UPS Panel: {}. O computador vai desligar.", "Lite UPS Panel: {}. The computer will shut down.", "Lite UPS Panel: {}. El equipo se apagará.", "Lite UPS Panel: {}. Il computer verrà spento.", "Lite UPS Panel : {}. L'ordinateur va s'arrêter."],
    ["comunicação com a UPS perdida em bateria", "UPS communication lost while on battery", "comunicación con la UPS perdida con batería", "comunicazione con l'UPS persa a batteria", "communication avec l'onduleur perdue sur batterie"],
    ["a UPS indica bateria no limite", "the UPS reports the battery at its limit", "la UPS indica que la batería está al límite", "l'UPS indica che la batteria è al limite", "l'onduleur indique que la batterie est à sa limite"],
    ["tempo máximo em bateria atingido", "maximum time on battery reached", "tiempo máximo con batería alcanzado", "tempo massimo a batteria raggiunto", "durée maximale sur batterie atteinte"],
    ["runtime restante baixo", "remaining runtime is low", "autonomía restante baja", "autonomia residua bassa", "autonomie restante faible"],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_complete_and_consistent() {
        for (i, r) in T.iter().enumerate() {
            assert!(r.iter().all(|c| !c.is_empty()), "linha {i} com coluna vazia: {}", r[0]);
            let n = r[0].matches("{}").count();
            for (l, c) in r.iter().enumerate() { assert_eq!(c.matches("{}").count(), n, "{{}} diferente em '{}' (coluna {l})", r[0]); }
            assert_eq!(T.iter().filter(|x| x[0] == r[0]).count(), 1, "chave repetida: {}", r[0]);
        }
    }

    #[test]
    fn every_tr_literal_in_the_sources_has_a_translation() {
        let srcs = [include_str!("ui.rs"), include_str!("ui_pages.rs"), include_str!("ui_win.rs"), include_str!("nav.rs"),
            include_str!("app.rs"), include_str!("shutdown.rs"), include_str!("monitor.rs")];
        let mut n = 0;
        for s in srcs {
            let mut rest = s;
            while let Some(i) = rest.find("tr(\"") {
                let after = &rest[i + 4..];
                // só chamadas isoladas a tr( (não `str(`, `attr(` …)
                let ok = rest[..i].chars().next_back().map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true);
                let mut key = String::new();
                let mut it = after.chars();
                while let Some(c) = it.next() {
                    match c {
                        '\\' => match it.next() { Some('n') => key.push('\n'), Some(o) => key.push(o), None => break },
                        '"' => break,
                        _ => key.push(c),
                    }
                }
                if ok { assert!(has(&key), "sem tradução: {key:?}"); n += 1; }
                rest = &rest[i + 4..];
            }
        }
        assert!(n > 40, "scanner encontrou só {n} chamadas");
    }

    #[test]
    fn indirect_keys_exist() {
        for k in ["Estado", "Eventos", "Energia", "Configuração", "UPS", "Bateria", "Rede elétrica", "Normal", "AC", "Em bateria",
            "Evento", "Falha ao desligar o computador", "Nunca", "Nenhum", "Passou", "Falhou"] {
            if k != "UPS" { assert!(has(k), "{k}"); }
        }
    }

    #[test]
    fn langid_mapping() {
        assert_eq!(from_langid(0x0816), Lang::Pt); // pt-PT
        assert_eq!(from_langid(0x0416), Lang::Pt); // pt-BR
        assert_eq!(from_langid(0x0409), Lang::En);
        assert_eq!(from_langid(0x0C0A), Lang::Es);
        assert_eq!(from_langid(0x0410), Lang::It);
        assert_eq!(from_langid(0x040C), Lang::Fr);
        assert_eq!(from_langid(0x0407), Lang::En); // alemão → inglês
    }

    #[test]
    fn fill_replaces_in_order() {
        assert_eq!(fill("a {} b {} c", &["1".into(), "2".into()]), "a 1 b 2 c");
    }
}
