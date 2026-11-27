//! Profile scripts on QuickJS, as FlClash runs them. FlClash: the rust_api `script` module,
//! lib/common/javascript.dart.

mod console;

use rquickjs::{CatchResultExt, Context, Function, Runtime, Value};
use serde_json::{Map, Value as Json};
use std::time::{Duration, Instant};

const ENTRY: &str = "main";
// Evaluation blocks applying the profile, so a looping script must be stopped.
const TIMEOUT: Duration = Duration::from_secs(10);
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;

pub fn evaluate(script: &str, config: &str) -> Result<String, String> {
    evaluate_within(script, config, TIMEOUT)
}

fn evaluate_within(script: &str, config: &str, timeout: Duration) -> Result<String, String> {
    let runtime = Runtime::new().map_err(|e| format!("{e}"))?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    let deadline = Instant::now() + timeout;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));

    let context = Context::full(&runtime).map_err(|e| format!("{e}"))?;
    context.with(|ctx| {
        console::install(&ctx).catch(&ctx).map_err(describe)?;
        ctx.eval::<Value, _>(script.as_bytes())
            .catch(&ctx)
            .map_err(describe)?;
        // A top-level `const main` is a lexical binding, not a property of globalThis.
        let entry: Function = ctx
            .eval(ENTRY.as_bytes())
            .map_err(|_| format!("script does not define {ENTRY}()"))?;
        let parsed: Value = ctx
            .json_parse(config)
            .catch(&ctx)
            .map_err(|_| "profile is not valid JSON".to_owned())?;
        let result: Value = entry.call((parsed,)).catch(&ctx).map_err(describe)?;
        let result = match result.as_promise() {
            Some(promise) => {
                promise
                    .finish::<Value>()
                    .catch(&ctx)
                    .map_err(|error| match error {
                        rquickjs::CaughtError::Error(rquickjs::Error::WouldBlock) => {
                            format!("{ENTRY}() returned a Promise that did not settle")
                        }
                        other => describe(other),
                    })?
            }
            None => result,
        };
        if result.is_undefined() || result.is_null() {
            return Ok(config.to_owned());
        }
        let json = ctx
            .json_stringify(result)
            .catch(&ctx)
            .map_err(describe)?
            .ok_or_else(|| format!("{ENTRY}() returned a value that is not JSON"))?;
        json.to_string().map_err(|e| format!("{e}"))
    })
}

fn describe(error: rquickjs::CaughtError<'_>) -> String {
    match error {
        rquickjs::CaughtError::Exception(exception) => {
            let message = exception.message().unwrap_or_else(|| exception.to_string());
            match exception.stack() {
                Some(stack) if !stack.trim().is_empty() => format!("{message}\n{}", stack.trim()),
                _ => message,
            }
        }
        other => other.to_string(),
    }
}

/// `handleEvaluate`: `proxy-providers` always present, and a mapping must come back.
pub fn handle_evaluate(script: &str, config: &Json) -> Result<Json, String> {
    let mut input = config.as_object().cloned().unwrap_or_default();
    if input.get("proxy-providers").is_none_or(Json::is_null) {
        input.insert("proxy-providers".into(), Json::Object(Map::new()));
    }
    let result = evaluate(script, &Json::Object(input).to_string())?;
    match serde_json::from_str::<Json>(&result) {
        Ok(v @ Json::Object(_)) => Ok(v),
        _ => Err("script did not return a configuration object".into()),
    }
}

/// `_outboundsOf`'s rules: the MATCH lines a scripted mapping ends on.
pub fn match_lines(config: &Json) -> Vec<String> {
    config
        .get("rules")
        .and_then(Json::as_array)
        .map(|rules| {
            rules
                .iter()
                .filter_map(Json::as_str)
                .filter(|r| {
                    r.split(',')
                        .next()
                        .is_some_and(|t| t.trim().eq_ignore_ascii_case("MATCH"))
                })
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(script: &str, config: Json) -> Result<Json, String> {
        evaluate(script, &config.to_string()).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn returns_the_object_main_produced() {
        let result = run(
            "function main(config) { config.mode = 'global'; return config }",
            json!({ "mode": "rule" }),
        )
        .unwrap();

        assert_eq!(result, json!({ "mode": "global" }));
    }

    #[test]
    fn accepts_a_lexically_declared_arrow_function_entry() {
        let result = run(
            "const main = (config) => { return config; }",
            json!({ "mode": "rule" }),
        )
        .unwrap();

        assert_eq!(result, json!({ "mode": "rule" }));
    }

    #[test]
    fn keeps_the_profile_when_main_returns_nothing() {
        let result = run("function main(config) {}", json!({ "mode": "rule" })).unwrap();

        assert_eq!(result, json!({ "mode": "rule" }));
    }

    #[test]
    fn resolves_a_promise_main_returns() {
        let result = run(
            "async function main(config) { config.mode = 'global'; return config }",
            json!({ "mode": "rule" }),
        )
        .unwrap();

        assert_eq!(result, json!({ "mode": "global" }));
    }

    #[test]
    fn reports_the_rejection_of_a_promise_main_returns() {
        let error = run(
            "async function main() { throw new Error('bad profile') }",
            json!({}),
        )
        .unwrap_err();

        assert!(error.contains("bad profile"), "{error}");
    }

    #[test]
    fn reports_a_promise_that_never_settles() {
        let error = run(
            "function main() { return new Promise(() => {}) }",
            json!({}),
        )
        .unwrap_err();

        assert!(error.contains("did not settle"), "{error}");
    }

    #[test]
    fn reports_a_missing_entry_point() {
        let error = run("const value = 1", json!({})).unwrap_err();

        assert!(error.contains("main()"), "{error}");
    }

    #[test]
    fn reports_the_message_and_stack_of_a_thrown_error() {
        let error = run(
            "function main() { throw new Error('bad profile') }",
            json!({}),
        )
        .unwrap_err();

        assert!(error.contains("bad profile"), "{error}");
        assert!(error.contains("main"), "{error}");
    }

    #[test]
    fn reports_a_syntax_error_with_its_location() {
        let error = run("function main( {", json!({})).unwrap_err();

        assert!(error.contains(":1:"), "{error}");
    }

    #[test]
    fn stops_a_script_that_never_finishes() {
        let started = Instant::now();
        let error = evaluate_within(
            "function main() { while (true) {} }",
            "{}",
            Duration::from_millis(100),
        )
        .unwrap_err();

        assert!(!error.is_empty());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn console_calls_do_not_stop_a_script() {
        let result = run(
            "function main(config) { console.log('a', 1); console.error({}); return config }",
            json!({ "mode": "rule" }),
        )
        .unwrap();

        assert_eq!(result, json!({ "mode": "rule" }));
    }

    #[test]
    fn supports_the_language_features_profile_scripts_use() {
        let script = r#"
            const regions = new Map([['HK', '🇭🇰']]);
            function main(config) {
              const names = [...new Set(config.proxies.map((proxy) => proxy.name))];
              const flag = regions.get('HK') ?? '';
              return {
                ...config,
                names: names.map((name) => `${flag} ${name}`.trim()),
                first: config.proxies?.[0]?.name ?? null,
                entries: Object.fromEntries(names.map((name, index) => [name, index])),
              };
            }
        "#;

        let result = run(
            script,
            json!({ "proxies": [{ "name": "a" }, { "name": "b" }, { "name": "a" }] }),
        )
        .unwrap();

        assert_eq!(result["names"], json!(["🇭🇰 a", "🇭🇰 b"]));
        assert_eq!(result["first"], json!("a"));
        assert_eq!(result["entries"], json!({ "a": 0, "b": 1 }));
    }

    #[test]
    fn evaluates_with_proxy_providers_and_wants_a_mapping_back() {
        let out = handle_evaluate(
            "function main(c) { c.seen = typeof c['proxy-providers']; return c }",
            &json!({"mode": "rule", "proxy-providers": null}),
        )
        .unwrap();
        assert_eq!(out["seen"], json!("object"));
        let err = handle_evaluate("function main() { return [1] }", &json!({})).unwrap_err();
        assert!(err.contains("configuration object"), "{err}");
    }

    #[test]
    fn keeps_the_match_lines() {
        let config = json!({"rules": ["DOMAIN,a.com,DIRECT", "match , 节点", 3, "MATCH,手动选择"]});
        assert_eq!(match_lines(&config), vec!["match , 节点", "MATCH,手动选择"]);
        assert!(match_lines(&json!({})).is_empty());
    }
}
