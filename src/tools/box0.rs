use super::ToolSpec;
use serde_json::json;

pub const DEFAULT_PERSPECTIVES: [&str; 3] = ["code", "reasoning", "main"];

const MAX_REPORT: usize = 8_000;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "box0",
        description: "Débat multi-agents (0box) : plusieurs perspectives IA débattent \
                      sur un sujet, puis MAIN synthétise et peut exécuter les actions \
                      (tâches, fichiers). Renvoie la transcription du débat.",
        parameters: json!({
            "type": "object",
            "properties": {
                "topic": { "type": "string", "description": "Sujet du débat." },
                "perspectives": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Perspectives (rôles), ex. [\"code\",\"reasoning\",\"main\"]. Défaut : code, reasoning, main."
                }
            },
            "required": ["topic"]
        }),
    }
}

pub fn parse_args(args: &str) -> Result<(String, Vec<String>), String> {
    let v: serde_json::Value =
        serde_json::from_str(args).map_err(|e| format!("arguments JSON invalides ({e})"))?;
    let topic = v
        .get("topic")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if topic.is_empty() {
        return Err("topic requis".to_string());
    }
    let perspectives: Vec<String> = v
        .get("perspectives")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .filter(|p: &Vec<String>| !p.is_empty())
        .unwrap_or_else(|| DEFAULT_PERSPECTIVES.iter().map(|s| s.to_string()).collect());
    Ok((topic, perspectives))
}

pub async fn run(args: &str) -> String {
    let (topic, perspectives) = match parse_args(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur 0box : {e}"),
    };

    let Some(rt) = super::runtime() else {
        return "Erreur 0box : contexte d'exécution absent (l'agent n'est pas initialisé). \
                Lance le débat avec `/0box <sujet>` dans la TUI ou `zer0 0box <sujet>`."
            .to_string();
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::event::UiEvent>();
    let setup = crate::tui::DebateSetup {
        settings: rt.settings.clone(),
        system: rt.system.clone(),
        options: rt.options.clone(),
        read_write: rt.read_write,
        code_exec: rt.code_exec,
        all_tools: rt.all_tools.clone(),
        ui: tx,
    };

    let perspectives_for_prompt = perspectives.clone();
    let topic_for_report = topic.clone();
    tokio::spawn(async move {
        crate::tui::run_0box_debate(topic, perspectives, setup).await;
    });

    let mut report = String::new();
    while let Some(ev) = rx.recv().await {
        if let crate::event::UiEvent::Info(t) = ev {
            report.push_str(&t);
            report.push('\n');
        }
    }

    if report.trim().is_empty() {
        return format!(
            "0box : débat terminé sans sortie exploitable sur « {topic_for_report} » (perspectives : {}).",
            perspectives_for_prompt.join(", ")
        );
    }
    truncate_report(report)
}

fn truncate_report(report: String) -> String {
    if report.len() <= MAX_REPORT {
        return report;
    }
    let cut = crate::util::floor_char_boundary(&report, report.len() - MAX_REPORT);
    format!("… (début tronqué)\n{}", &report[cut..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_to_three_perspectives() {
        let (topic, p) = parse_args(r#"{"topic":"conception d'un CLI"}"#).unwrap();
        assert_eq!(topic, "conception d'un CLI");
        assert_eq!(p, vec!["code", "reasoning", "main"]);
    }

    #[test]
    fn parse_accepts_custom_perspectives() {
        let (_, p) = parse_args(r#"{"topic":"x","perspectives":["main","writing"]}"#).unwrap();
        assert_eq!(p, vec!["main", "writing"]);
    }

    #[test]
    fn parse_rejects_empty_topic_and_bad_json() {
        assert!(parse_args(r#"{"topic":"  "}"#).is_err());
        assert!(parse_args("pas du json").is_err());
    }

    #[test]
    fn parse_ignores_empty_perspectives_list() {
        let (_, p) = parse_args(r#"{"topic":"x","perspectives":[]}"#).unwrap();
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn short_report_is_untouched() {
        let r = "débat court".to_string();
        assert_eq!(truncate_report(r.clone()), r);
    }

    #[test]
    fn truncate_report_never_panics_on_multibyte() {
        for offset in 0..4 {
            let mut r = String::from("é");
            r.push_str(&"a".repeat(MAX_REPORT - 1 + offset));
            let out = truncate_report(r);
            assert!(out.starts_with("… (début tronqué)"));
            assert!(out.len() <= MAX_REPORT + 32);
        }
    }

    #[test]
    fn truncate_report_keeps_the_tail() {
        let mut r = String::from("DEBUT");
        r.push_str(&"x".repeat(MAX_REPORT));
        r.push_str("FIN");
        let out = truncate_report(r);
        assert!(out.ends_with("FIN"));
        assert!(!out.contains("DEBUT"));
    }
}
