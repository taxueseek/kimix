use agent_client_protocol as acp;
use serde::{Deserialize, Serialize};
use serde_json::value::to_raw_value;
use std::sync::Arc;

pub use kimix_shell_base::session_types::{ExtMethodError, ExtMethodResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Empty {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize)]
    struct TestData {
        nodes: Vec<String>,
        truncated: bool,
    }

    #[test]
    fn test_ext_method_result_serialization() {
        // Success case
        let data = TestData {
            nodes: vec!["test".to_string()],
            truncated: false,
        };
        let success: ExtMethodResult<TestData> = ExtMethodResult::from_result::<String>(Ok(data));
        let json = serde_json::to_value(&success).unwrap();

        // Should have "result" field
        assert!(
            json.get("result").is_some(),
            "Success case should have 'result' field"
        );
        assert!(
            json.get("error").is_none(),
            "Success case should not have 'error' field"
        );

        let result = json.get("result").unwrap();
        assert!(
            result.get("nodes").is_some(),
            "Result should have 'nodes' field"
        );

        println!(
            "Success JSON: {}",
            serde_json::to_string_pretty(&success).unwrap()
        );

        // Error case
        let error: ExtMethodResult<TestData> =
            ExtMethodResult::from_result::<&str>(Err("test error"));
        let json = serde_json::to_value(&error).unwrap();

        // Should have "result": null and "error" field
        assert!(
            json.get("result").is_some(),
            "Error case should have 'result' field"
        );
        assert_eq!(json.get("result").unwrap(), &serde_json::Value::Null);
        assert!(
            json.get("error").is_some(),
            "Error case should have 'error' field"
        );

        println!(
            "Error JSON: {}",
            serde_json::to_string_pretty(&error).unwrap()
        );
    }
}
