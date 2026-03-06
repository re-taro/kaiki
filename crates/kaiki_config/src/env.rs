use crate::ConfigError;

/// Expand environment variables in a string.
///
/// Supports three patterns:
/// - `${VAR}` - expand to the value of environment variable VAR
/// - `$VAR` - expand to the value of environment variable VAR (word boundary)
/// - `$$` - escape to a literal `$`
pub fn expand_env_vars(input: &str) -> Result<String, ConfigError> {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            if i + 1 < chars.len() && chars[i + 1] == '$' {
                // $$ → literal $
                result.push('$');
                i += 2;
            } else if i + 1 < chars.len() && chars[i + 1] == '{' {
                // ${VAR} pattern
                if let Some(end) = chars[i + 2..].iter().position(|&c| c == '}') {
                    let var_name: String = chars[i + 2..i + 2 + end].iter().collect();
                    let value = std::env::var(&var_name)
                        .map_err(|_| ConfigError::EnvVar(var_name.clone()))?;
                    result.push_str(&value);
                    i = i + 3 + end;
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
            } else if i + 1 < chars.len()
                && (chars[i + 1].is_ascii_alphanumeric() || chars[i + 1] == '_')
            {
                // $VAR pattern
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                {
                    end += 1;
                }
                let var_name: String = chars[start..end].iter().collect();
                let value =
                    std::env::var(&var_name).map_err(|_| ConfigError::EnvVar(var_name.clone()))?;
                result.push_str(&value);
                i = end;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_braced_var() {
        // SAFETY: test-only, single-threaded test runner
        unsafe { std::env::set_var("KAIKI_TEST_VAR", "hello") };
        assert_eq!(expand_env_vars("${KAIKI_TEST_VAR}").unwrap(), "hello");
    }

    #[test]
    fn test_expand_unbraced_var() {
        // SAFETY: test-only, single-threaded test runner
        unsafe { std::env::set_var("KAIKI_TEST_VAR2", "world") };
        assert_eq!(expand_env_vars("$KAIKI_TEST_VAR2").unwrap(), "world");
    }

    #[test]
    fn test_expand_escape() {
        assert_eq!(expand_env_vars("$$").unwrap(), "$");
    }

    #[test]
    fn test_no_expansion() {
        assert_eq!(expand_env_vars("no vars here").unwrap(), "no vars here");
    }

    #[test]
    fn test_missing_var() {
        let result = expand_env_vars("${KAIKI_NONEXISTENT_VAR_XYZ}");
        assert!(result.is_err());
    }
}
