//! One test binary for the crate. Every former `tests/*.rs` is a module here;
//! add a `mod` line for a new file. One binary links the dependency tree once
//! instead of once per file, which is most of a test build's cost and disk.

#[path = "../server/auth_13_cache_key.rs"]
mod auth_13_cache_key;
#[path = "../server/auth_session_layer.rs"]
mod auth_session_layer;
#[path = "../server/auth_05_magic.rs"]
mod auth_05_magic;
#[path = "../server/auth_03_jwt_return.rs"]
mod auth_03_jwt_return;
#[path = "../server/auth_13_cdn.rs"]
mod auth_13_cdn;
#[path = "../server/ana_02_public_schema.rs"]
mod ana_02_public_schema;
#[path = "../server/auth_14_bot_protection.rs"]
mod auth_14_bot_protection;
#[path = "../server/auth_07_mixed.rs"]
mod auth_07_mixed;
#[path = "../server/api_50_download.rs"]
mod api_50_download;

#[path = "../build/api_03_pages_render.rs"]
mod api_03_pages_render;
#[path = "../build/auth_01.rs"]
mod auth_01;
#[path = "../config/cfg_01.rs"]
mod cfg_01;
#[path = "../config/cfg_03_presets.rs"]
mod cfg_03_presets;
#[path = "../config/cfg_04.rs"]
mod cfg_04;
#[path = "../config/cfg_30_navigation.rs"]
mod cfg_30_navigation;
#[path = "../build/cfg_30_sidebar.rs"]
mod cfg_30_sidebar;
#[path = "../build/cfg_64.rs"]
mod cfg_64;
#[path = "../build/cfg_65.rs"]
mod cfg_65;
#[path = "../cli/cfg_83.rs"]
mod cfg_83;
#[path = "../config/cfg_96_operators.rs"]
mod cfg_96_operators;
#[path = "../config/cfg_90_diagnostics.rs"]
mod cfg_90_diagnostics;
#[path = "../build/cfg_90_rules.rs"]
mod cfg_90_rules;
#[path = "../build/cmp_80_gates.rs"]
mod cmp_80_gates;
#[path = "../config/cfg_91.rs"]
mod cfg_91;
#[path = "../config/cfg_99_context_repos.rs"]
mod cfg_99_context_repos;
#[path = "../config/cfg_94_schema.rs"]
mod cfg_94_schema;
#[path = "../cli/cfg_90_validate.rs"]
mod cfg_90_validate;
#[path = "../cli/cli_13_schema.rs"]
mod cli_13_schema;
#[path = "../cli/cli_14_migrate.rs"]
mod cli_14_migrate;
#[path = "../cli/cfg_93_env.rs"]
mod cfg_93_env;
#[path = "../cli/cli_02_dev.rs"]
mod cli_02_dev;
#[path = "../cli/cli_03_build.rs"]
mod cli_03_build;
#[path = "../cli/cli_05_format.rs"]
mod cli_05_format;
#[path = "../build/cm_120_changelog.rs"]
mod cm_120_changelog;
#[path = "../build/cm_15_functions.rs"]
mod cm_15_functions;
#[path = "../build/cm_36_links.rs"]
mod cm_36_links;
#[path = "../build/cm_70_snippets.rs"]
mod cm_70_snippets;
#[path = "../build/api_10_spec_pages.rs"]
mod api_10_spec_pages;
#[path = "../build/rfc_0914_retained.rs"]
mod rfc_0914_retained;
#[path = "../build/ver_77_reviewed.rs"]
mod ver_77_reviewed;
#[path = "../build/auth_53_region_index.rs"]
mod auth_53_region_index;
#[path = "../build/cm_80_hidden.rs"]
mod cm_80_hidden;
#[path = "../build/cm_82_redirects.rs"]
mod cm_82_redirects;
#[path = "../build/cm_84_files.rs"]
mod cm_84_files;
#[path = "../git/git_01_github.rs"]
mod git_01_github;
#[path = "../git/ver_77_last_author.rs"]
mod ver_77_last_author;
#[path = "../server/git_20_deploy.rs"]
mod git_20_deploy;
#[path = "../server/git_21.rs"]
mod git_21;
#[path = "../server/git_24_queue.rs"]
mod git_24_queue;
#[path = "../server/git_40_rollback.rs"]
mod git_40_rollback;
#[path = "../server/git_41.rs"]
mod git_41;
#[path = "../build/cmp_82_variants.rs"]
mod cmp_82_variants;
#[path = "../hosting/host_01_matrix.rs"]
mod host_01_matrix;
#[path = "../hosting/host_02.rs"]
mod host_02;
#[path = "../hosting/host_04_proxies.rs"]
mod host_04_proxies;
#[path = "../server/host_20_domains.rs"]
mod host_20_domains;
#[path = "../server/host_05.rs"]
mod host_05;
#[path = "../server/host_07_jobs.rs"]
mod host_07_jobs;
#[path = "../docs/mig_22.rs"]
mod mig_22;
#[path = "../docs/nfr_70.rs"]
mod nfr_70;
#[path = "../web/nojs.rs"]
mod nojs;
#[path = "../build/src_05_index_written.rs"]
mod src_05_index_written;
#[path = "../build/src_05_index_warm.rs"]
mod src_05_index_warm;
#[path = "../build/rx_02_assets.rs"]
mod rx_02_assets;
#[path = "../build/rx_110_csp.rs"]
mod rx_110_csp;
#[path = "../build/rx_111_base_path.rs"]
mod rx_111_base_path;
#[path = "../build/rx_112_static.rs"]
mod rx_112_static;
#[path = "../budget/rx_12.rs"]
mod rx_12;
#[path = "../server/rx_13.rs"]
mod rx_13;
#[path = "../build/rx_13_headers.rs"]
mod rx_13_headers;
#[path = "../budget/rx_14_transfer.rs"]
mod rx_14_transfer;
#[path = "../server/rx_50_feedback.rs"]
mod rx_50_feedback;
#[path = "../budget/thm_31.rs"]
mod thm_31;
#[path = "../web/reader.rs"]
mod web_reader;
#[path = "../verify/truth_graph.rs"]
mod truth_graph;
#[path = "../verify/ver_01_attributes.rs"]
mod ver_01_attributes;
#[path = "../verify/runners/rust.rs"]
mod ver_02_06_rust;
#[path = "../verify/runners/shell.rs"]
mod ver_02_1_shell;
#[path = "../verify/ver_03_isolation.rs"]
mod ver_03_isolation;
#[path = "../server/no_caller_ratchet.rs"]
mod no_caller_ratchet;
#[path = "../server/mount.rs"]
mod mount;
#[path = "../server/org_28.rs"]
mod org_28;
#[path = "../verify/ver_20_types.rs"]
mod ver_20_types;
#[path = "../verify/ver_21_deps.rs"]
mod ver_21_deps;
#[path = "../verify/ver_22_snapshots.rs"]
mod ver_22_snapshots;
#[path = "../verify/ver_25_commands.rs"]
mod ver_25_commands;
#[path = "../verify/ver_12_spec_drift.rs"]
mod ver_12_spec_drift;
#[path = "../verify/ver_26_trust.rs"]
mod ver_26_trust;
#[path = "../verify/ver_23_drift.rs"]
mod ver_23_drift;
#[path = "../web/editor.rs"]
mod web_editor;
#[path = "../editor/segments.rs"]
mod editor_segments;
#[path = "../editor/ed_03_roundtrip.rs"]
mod ed_03_roundtrip;
#[path = "../editor/ed_13_media.rs"]
mod ed_13_media;
#[path = "../server/ed_75_roles.rs"]
mod ed_75_roles;
#[path = "../editor/ed_73_messages.rs"]
mod ed_73_messages;
#[path = "../editor/ed_01_model.rs"]
mod ed_01_model;
#[path = "../server/host_07_worker.rs"]
mod host_07_worker;
#[path = "../server/dashboard_read_ratchet.rs"]
mod dashboard_read_ratchet;
#[path = "../server/ast_11_assistant_endpoint.rs"]
mod ast_11_assistant_endpoint;
#[path = "../server/ast_10_tools.rs"]
mod ast_10_tools;
#[path = "../server/ast_01_embed_job.rs"]
mod ast_01_embed_job;
#[path = "../editor/ed_01_component_forms.rs"]
mod ed_01_component_forms;

#[path = "../server/mcp_01_server.rs"]
mod mcp_01_server;

#[path = "../server/mcp_04_stdio.rs"]
mod mcp_04_stdio;
#[path = "../server/ana_02_dashboard_mount.rs"]
mod ana_02_dashboard_mount;
#[path = "../server/rest_04_search.rs"]
mod rest_04_search;
#[path = "../server/work_registry_union.rs"]
mod work_registry_union;
#[path = "../server/api_41_proxy.rs"]
mod api_41_proxy;
#[path = "../server/ana_20_search_events.rs"]
mod ana_20_search_events;
#[path = "../server/agt_02_research.rs"]
mod agt_02_research;
#[path = "../server/agt_02_research_job.rs"]
mod agt_02_research_job;
#[path = "../server/work_orphan_starvation.rs"]
mod work_orphan_starvation;
#[path = "../server/ver_77_reminders.rs"]
mod ver_77_reminders;

// WP-25 Agent runtime. `support` is the shared fixture; the rest are one file per
// acceptance row.
#[path = "../agent/support.rs"]
mod agent_support;
#[path = "../agent/agt_01_inputs.rs"]
mod agt_01_inputs;
#[path = "../agent/agt_02_loop.rs"]
mod agt_02_loop;
#[path = "../agent/agt_02_research_seam.rs"]
mod agt_02_research_seam;
#[path = "../agent/agt_02_research_retrieval.rs"]
mod agt_02_research_retrieval;
#[path = "../agent/agt_03_tools.rs"]
mod agt_03_tools;
#[path = "../agent/agt_04_trust.rs"]
mod agt_04_trust;
#[path = "../agent/agt_05_record.rs"]
mod agt_05_record;
#[path = "../agent/agt_06_gates.rs"]
mod agt_06_gates;
#[path = "../agent/agt_20_policy.rs"]
mod agt_20_policy;
#[path = "../agent/agt_30_agentsmd.rs"]
mod agt_30_agentsmd;
#[path = "../agent/agt_41_followup.rs"]
mod agt_41_followup;
#[path = "../agent/agt_42_bugs.rs"]
mod agt_42_bugs;

#[path = "../hosting/fixture_isolation.rs"]
mod fixture_isolation;

#[path = "../web/dashboard.rs"]
mod dashboard;
