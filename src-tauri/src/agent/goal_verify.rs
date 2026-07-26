//! goal 完成的 score-based 逐条验证：完成判据逐条过评审模型，全过才允许 complete。
//! 评审模型优先 review 角色绑定（独立视角），未配置回落当前会话模型（自证弱于独立评审，但零配置可用）。
//! 评审调用失败/输出不可解析按「本次 complete 拒绝」处理（可重试），不降级为弱校验静默放行。

use futures::StreamExt;

use crate::llm::{Delta, LlmClient, Message, ModelRef};

const EVIDENCE_CAP: usize = 8000;
const JUDGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq)]
pub struct CriterionScore {
    pub criterion: String,
    pub pass: bool,
    pub reason: String,
}

/// 判据文本拆条：非空行剥列表前缀（- / * / 1. / 1) / - [ ]）。
pub fn split_criteria(criteria: &str) -> Vec<String> {
    criteria
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let l = l.trim_start_matches("- [ ]").trim_start_matches("- [x]").trim_start_matches("- [X]");
            let l = l.trim_start_matches(['-', '*']).trim_start();
            // 数字有序前缀：1. / 2)
            let l = match l.find(['.', ')']) {
                Some(i) if i <= 3 && l[..i].chars().all(|c| c.is_ascii_digit()) && !l[..i].is_empty() => l[i + 1..].trim_start(),
                _ => l,
            };
            l.to_string()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

/// 评审输出解析：宽容截取首个 [ 到末个 ]（模型爱在 JSON 外包废话），结构不符返回 None。
pub fn parse_scores(text: &str) -> Option<Vec<CriterionScore>> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    if end <= start {
        return None;
    }
    #[derive(serde::Deserialize)]
    struct Raw {
        criterion: String,
        pass: bool,
        #[serde(default)]
        reason: String,
    }
    let raw: Vec<Raw> = serde_json::from_str(&text[start..=end]).ok()?;
    if raw.is_empty() {
        return None;
    }
    Some(raw.into_iter().map(|r| CriterionScore { criterion: r.criterion, pass: r.pass, reason: r.reason }).collect())
}

/// 逐条评审：每条判据一个 pass/reason，条数必须与判据数一致（漏条 = 评审不可信，按失败重试）。
pub async fn score_completion(
    model: &ModelRef,
    store: &crate::auth::credential::AuthStore,
    objective: &str,
    criteria: &str,
    evidence: &str,
) -> Result<Vec<CriterionScore>, String> {
    let items = split_criteria(criteria);
    if items.is_empty() {
        return Err("completion_criteria 拆不出判据条目，无法逐条验证".into());
    }
    let evidence_capped: String = evidence.chars().take(EVIDENCE_CAP).collect();
    let numbered = items.iter().enumerate().map(|(i, c)| format!("{}. {}", i + 1, c)).collect::<Vec<_>>().join("\n");
    let messages = vec![
        Message::system(
            "You are a strict completion verifier for a coding agent's goal. \
             Score each completion criterion against the evidence. \
             Reply with ONLY a JSON array, one object per criterion in the same order: \
             [{\"criterion\": \"...\", \"pass\": true, \"reason\": \"...\"}]. \
             pass=true only when the evidence concretely demonstrates the criterion \
             (commands actually run with shown output, files actually changed, tests actually green). \
             Vague claims, intentions, and partial results must fail.",
        ),
        Message::user(format!("Objective: {objective}\n\nCompletion criteria:\n{numbered}\n\nEvidence:\n{evidence_capped}")),
    ];
    let collect = async {
        let mut text = String::new();
        let mut stream = LlmClient::stream(model, &messages, store);
        while let Some(delta) = stream.next().await {
            match delta {
                Delta::Text(t) => text.push_str(&t),
                Delta::Error(e) => return Err(e),
                _ => {}
            }
        }
        Ok(text)
    };
    let text: String =
        tokio::time::timeout(JUDGE_TIMEOUT, collect).await.map_err(|_| "completion verification timed out (60s)".to_string())??;
    let scores = parse_scores(&text).ok_or_else(|| "completion verification returned unparseable scores".to_string())?;
    if scores.len() != items.len() {
        return Err(format!("completion verification scored {}/{} criteria, retry", scores.len(), items.len()));
    }
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_criteria_strips_list_prefixes() {
        let c = "- cargo test 全绿\n* dmg < 20MB\n1. 文档更新\n2) 无警告\n- [ ] 可选项\n裸行判据";
        let items = split_criteria(c);
        assert_eq!(items, vec!["cargo test 全绿", "dmg < 20MB", "文档更新", "无警告", "可选项", "裸行判据"]);
    }

    #[test]
    fn parse_scores_tolerates_prose_wrapper() {
        let text = "以下是评审结果：\n[{\"criterion\":\"a\",\"pass\":true,\"reason\":\"ok\"},{\"criterion\":\"b\",\"pass\":false}]\n以上。";
        let scores = parse_scores(text).expect("应解析成功");
        assert_eq!(scores.len(), 2);
        assert!(scores[0].pass);
        assert!(!scores[1].pass);
        assert_eq!(scores[1].reason, "");
    }

    #[test]
    fn parse_scores_rejects_garbage() {
        assert!(parse_scores("没有 JSON").is_none());
        assert!(parse_scores("[]").is_none());
        assert!(parse_scores("[{\"criterion\":\"a\"}]").is_none()); // 缺 pass 字段
    }
}
