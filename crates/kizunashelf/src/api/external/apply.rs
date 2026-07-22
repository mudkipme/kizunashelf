//! Entity-scoped review and apply of an external candidate. `review` resolves a
//! candidate's schema mapping against an existing entity and returns per-value
//! default selections; `apply` merges the confirmed fields into frontmatter and
//! splices the confirmed body sections under their headings. Both re-run the
//! schema mapping server-side — clients never send resolved values, only the
//! candidate and the selected keys — so what a match *means* can't drift between
//! runtimes. This used to live in each client (`external-metadata.ts` on the
//! web, `ExternalCandidateMapper` on iOS).

use super::mapping::match_candidate;
use crate::api::error::{ApiError, ApiResult};
use crate::api::mutations::{check_revision, edit_entity_document, type_config_or_err, EntityPath};
use crate::api::state::{get_library, require_content_writes, AppState};
use crate::contract::{
    EntityMutationResponse, ExternalApplyRequest, ExternalReviewField, ExternalReviewRequest,
    ExternalReviewResponse, ExternalReviewSection, MappedBodySection, MappedFieldValue,
};
use crate::library::load_entity;
use crate::markdown::{find_section, splice_section};
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) async fn review_external_candidate(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<ExternalReviewRequest>,
) -> ApiResult<ExternalReviewResponse> {
    let library = get_library(&state).await?;
    let Some(record) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let type_config = type_config_or_err(&library.config, &record.summary.entity_type)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let entity = load_entity(&library.config, vfs.as_ref(), &record.summary).await?;

    let candidate =
        super::enrich_candidate_for_type(&state, request.candidate, type_config, None).await?;
    let resolved = match_candidate(candidate.clone(), type_config);
    let fields = resolved
        .fields
        .into_iter()
        .map(|entry| {
            let current = entity.frontmatter.get(&entry.field);
            let (selected, locked) = field_selection(&entry, current);
            ExternalReviewField {
                current: current.cloned().unwrap_or(Value::Null),
                field: entry.field,
                value: entry.value,
                source: entry.source,
                external_field: entry.external_field,
                has_value: entry.has_value,
                selected,
                locked,
            }
        })
        .collect();
    let sections = resolved
        .body_sections
        .into_iter()
        .map(|entry| {
            let (selected, locked) = section_selection(&entry, &entity.body);
            ExternalReviewSection {
                exists: find_section(&entity.body, &entry.heading).is_some(),
                key: entry.key,
                heading: entry.heading,
                source: entry.source,
                external_field: entry.external_field,
                markdown: entry.markdown,
                has_value: entry.has_value,
                selected,
                locked,
            }
        })
        .collect();
    Ok(Json(ExternalReviewResponse {
        entity_type: resolved.entity_type,
        // The enriched candidate (its enrichment marker already consumed):
        // clients echo it to apply, which then passes it through unchanged
        // instead of resolving provider detail a second time.
        candidate,
        fields,
        sections,
    }))
}

pub(crate) async fn apply_external_candidate(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<ExternalApplyRequest>,
) -> ApiResult<EntityMutationResponse> {
    if request.fields.is_empty() && request.sections.is_empty() {
        return Err(ApiError::bad_request("Nothing selected to apply"));
    }
    let library = require_content_writes(&state).await?;
    let Some(record) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    check_revision(&request.revision, &record.revision)?;
    let type_config = type_config_or_err(&library.config, &record.summary.entity_type)?;
    let source_rel = record.summary.path.clone();

    let vfs = state.vault_vfs(&library.config.vault_root);
    let candidate =
        super::enrich_candidate_for_type(&state, request.candidate, type_config, None).await?;
    let resolved = match_candidate(candidate, type_config);
    let selected_fields: HashSet<&str> = request.fields.iter().map(String::as_str).collect();
    let selected_sections: HashSet<&str> = request.sections.iter().map(String::as_str).collect();
    edit_entity_document(
        &state,
        vfs.as_ref(),
        &source_rel,
        &request.revision,
        |document| {
            for entry in &resolved.fields {
                if entry.has_value && selected_fields.contains(entry.field.as_str()) {
                    document
                        .frontmatter
                        .insert(entry.field.clone(), entry.value.clone());
                }
            }
            for section in &resolved.body_sections {
                if section.has_value && selected_sections.contains(section.key.as_str()) {
                    document.body =
                        splice_section(&document.body, &section.heading, &section.markdown);
                }
            }
            // An entity with no body still gets the candidate's brief as a
            // starting point (only when nothing else filled the body).
            if document.body.trim().is_empty() {
                if let Some(brief) = resolved
                    .candidate
                    .brief
                    .as_deref()
                    .map(str::trim)
                    .filter(|brief| !brief.is_empty())
                {
                    document.body = brief.to_string();
                }
            }
            Ok(())
        },
    )
    .await?;
    let reloaded = get_library(&state).await?;
    let record = reloaded
        .record_by_id(&path.id)
        .ok_or_else(|| ApiError::not_found("Updated entity was not indexed"))?;
    let entity = load_entity(&reloaded.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(EntityMutationResponse {
        entity,
        updated_links: None,
    }))
}

/// Default checked/locked state for a candidate field against the current value.
/// Equality is checked before the external-ref rule, so re-matching the same
/// candidate leaves an already-set ref locked *off* rather than on.
fn field_selection(entry: &MappedFieldValue, current: Option<&Value>) -> (bool, bool) {
    if !entry.has_value {
        return (false, true);
    }
    if values_equal(&entry.value, current) {
        return (false, true);
    }
    // The external ref (no source metadata field — its value is the candidate
    // URL) anchors future refreshes, so applying a match always applies it.
    if entry.external_field.is_none() {
        return (true, true);
    }
    // Fill blanks by default; leave populated fields for the user to opt into.
    (is_empty_value(current), false)
}

/// Body-section counterpart: an absent section defaults on; identical existing
/// content is locked off; differing existing content stays off but editable.
fn section_selection(entry: &MappedBodySection, body: &str) -> (bool, bool) {
    if !entry.has_value {
        return (false, true);
    }
    match current_section_markdown(body, &entry.heading) {
        None => (true, false),
        Some(current) if current == entry.markdown.trim() => (false, true),
        Some(_) => (false, false),
    }
}

/// The trimmed Markdown currently under `heading`, or `None` when absent.
fn current_section_markdown(body: &str, heading: &str) -> Option<String> {
    let section = find_section(body, heading)?;
    Some(body[section.content_start..section.end].trim().to_string())
}

fn is_empty_value(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.trim().is_empty(),
        Some(Value::Array(items)) => items.is_empty(),
        Some(_) => false,
    }
}

/// Structural equality of a candidate value against the current value; both
/// empty counts as equal. Anything not provably equal falls through as
/// "differs", so at worst a same-valued field stays editable rather than locked.
fn values_equal(incoming: &Value, current: Option<&Value>) -> bool {
    if is_empty_value(Some(incoming)) && is_empty_value(current) {
        return true;
    }
    current.is_some_and(|current| incoming == current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(value: Value, external_field: Option<&str>) -> MappedFieldValue {
        MappedFieldValue {
            field: "field".to_string(),
            has_value: match &value {
                Value::Null => false,
                Value::String(text) => !text.trim().is_empty(),
                Value::Array(items) => !items.is_empty(),
                _ => true,
            },
            value,
            source: "tmdb".to_string(),
            external_field: external_field.map(str::to_string),
        }
    }

    fn section(markdown: &str) -> MappedBodySection {
        MappedBodySection {
            key: "Summary:tmdb:overview".to_string(),
            heading: "Summary".to_string(),
            source: "tmdb".to_string(),
            external_field: "overview".to_string(),
            markdown: markdown.to_string(),
            has_value: !markdown.trim().is_empty(),
        }
    }

    #[test]
    fn valueless_and_no_op_fields_are_locked_off() {
        assert_eq!(
            field_selection(&entry(Value::Null, Some("f")), None),
            (false, true)
        );
        assert_eq!(
            field_selection(&entry(json!("same"), Some("f")), Some(&json!("same"))),
            (false, true)
        );
        // Both-empty counts as equal even across shapes.
        assert_eq!(
            field_selection(&entry(json!([]), Some("f")), Some(&json!(""))),
            (false, true)
        );
    }

    #[test]
    fn the_external_ref_is_locked_on_unless_already_set() {
        let ref_entry = entry(json!("https://example.test/1"), None);
        assert_eq!(field_selection(&ref_entry, None), (true, true));
        // Re-matching the same candidate: equality wins, so the ref locks off.
        assert_eq!(
            field_selection(&ref_entry, Some(&json!("https://example.test/1"))),
            (false, true)
        );
    }

    #[test]
    fn blanks_default_on_and_populated_fields_default_off() {
        let incoming = entry(json!("New Value"), Some("f"));
        assert_eq!(field_selection(&incoming, None), (true, false));
        assert_eq!(field_selection(&incoming, Some(&json!(""))), (true, false));
        assert_eq!(
            field_selection(&incoming, Some(&json!("Hand-entered"))),
            (false, false)
        );
    }

    #[test]
    fn section_defaults_follow_the_current_body() {
        let incoming = section("A long voyage.");
        // Absent from the body: on. Identical: locked off. Differing: off, editable.
        assert_eq!(
            section_selection(&incoming, "No headings here."),
            (true, false)
        );
        assert_eq!(
            section_selection(&incoming, "## Summary\n\nA long voyage.\n"),
            (false, true)
        );
        assert_eq!(
            section_selection(&incoming, "## Summary\n\nMy own words.\n"),
            (false, false)
        );
    }
}
