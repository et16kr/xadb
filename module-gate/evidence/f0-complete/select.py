import subprocess
m='storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
groups={
'status_init::status_flush::resident::tests::':['resident_status_staged_key_rejects_other_slot','resident_status_completion_rejects_other_slot','resident_status_late_semantic_failure_releases_whole_read'],
'plan4_pages::tests::':['g5_clean_lifecycle','g5_canonical_source_boundaries','g5_clean_invariant_preflight','g5_residency_state_boundaries','g5_load_io_boundaries'],
'plan4_pages::retention::tests::lifecycle::':['retention_live_reader_boundaries','retention_live_latest_load','retention_live_writer_boundaries','retention_live_activation','retention_live_flush_boundaries'],
'plan4_pages::retention::free_undo_tests::':['free_undo_original_owner_gap_cancel_reuse_flush_reload','free_undo_new_role_preflight_and_install_failure_boundaries'],
'plan4_recovery::tests::free_undo::':['free_undo_reserved_recovery_uses_original_scope_cancel'],
'plan4_pages::retention::row::census_tests::':['s2w_c04_live_and_recovered_peaks_unchanged'],
'recovery_prefix::effect::tests::':['pending_and_healthy_corruption_boundaries'],
'status_init::status_flush::tests::':['status_flush_rf'],
'status_init::status_flush::mixed::tests::':['mixed_owner_status_first_dwb_error_keeps_dirty_obligation','mixed_owner_status_init_allocator_image_unwind','mixed_owner_status_init_stage_owned_unwind']}
selectors=[m+g+s for g,ss in groups.items() for s in ss]+[m+'status_init::status_flush::resident::tests::resident_status_cut_allocator']
subprocess.run(['python3','/workspace/scratch/f0-results/run.py','baseline',*selectors],check=True)
