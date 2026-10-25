"""Fixed kernel V3 and finite P0 evidence profiles, declared before long execution.

The durable literal inventory is the source-defined V2 corpus, copied verbatim
from its profile receipt; it is not a discovered subset of a V3 result.
"""

DURABLE_LANES = [{'lane': 'durable_request',
  'strict_count': 214,
  'strict_families': {'alias': 2,
                      'boundary': 2,
                      'concatenation': 3,
                      'directory': 3,
                      'eof': 1,
                      'fault_plan': 1,
                      'gate': 8,
                      'identity': 5,
                      'json': 3,
                      'length': 3,
                      'page': 2,
                      'statement': 2,
                      'truncation': 178,
                      'utf8': 1},
  'valid': ['configure_engine',
            'configure_fault',
            'create_database',
            'open_database',
            'backup',
            'close_database',
            'checkpoint',
            'collect_garbage',
            'metrics',
            'durability_metrics',
            'restore_database',
            'configure_gate',
            'create_table',
            'drop_table',
            'begin_read_only',
            'begin_read_write',
            'lookup',
            'bind_row',
            'statement',
            'statement_empty',
            'statement_limit',
            'read',
            'open_scan',
            'next_page',
            'close_scan',
            'commit',
            'rollback',
            'cancel']},
 {'lane': 'durable_response',
  'strict_count': 216,
  'strict_families': {'boundary': 2,
                      'concatenation': 3,
                      'eof': 1,
                      'error_catalog': 2,
                      'gate': 2,
                      'identity': 5,
                      'json': 3,
                      'length': 3,
                      'metrics': 6,
                      'truncation': 188,
                      'utf8': 1},
  'valid': ['ready',
            'done',
            'table_lookup',
            'table_create',
            'begun',
            'statement',
            'row_some',
            'row_none',
            'page',
            'page_empty',
            'committed_read',
            'committed_write',
            'checkpoint',
            'backup',
            'garbage_collected',
            'metrics',
            'durability_metrics_off',
            'durability_metrics_full',
            'transaction_error_XADB-ENGINE-0001',
            'transaction_error_XADB-ENGINE-0002',
            'transaction_error_XADB-ENGINE-0003',
            'transaction_error_XADB-ENGINE-0004',
            'transaction_error_XADB-ENGINE-0005',
            'transaction_error_XADB-ENGINE-0006',
            'transaction_error_XADB-ENGINE-0007',
            'transaction_error_XADB-ENGINE-0008',
            'transaction_error_XADB-ENGINE-0009',
            'transaction_error_ReadOnly',
            'transaction_error_SerializationConflict',
            'transaction_error_Cancelled',
            'transaction_error_Poisoned',
            'transaction_error_Fenced',
            'transaction_error_Closed',
            'storage_error_InvalidInput',
            'storage_error_Corruption',
            'storage_error_UnsupportedFormat',
            'storage_error_LimitExceeded',
            'storage_error_QuotaExceeded',
            'storage_error_Io',
            'storage_error_Busy',
            'storage_error_Fenced',
            'storage_error_IncompleteCreate',
            'storage_error_Transaction']},
 {'lane': 'fault_observation',
  'strict_count': 216,
  'strict_families': {'boundary': 2,
                      'concatenation': 3,
                      'cut_catalog': 36,
                      'eof': 1,
                      'identity': 3,
                      'json': 3,
                      'length': 3,
                      'truncation': 143,
                      'utf8': 1,
                      'witness': 21},
  'valid': ['connected',
            'released_continue',
            'released_return_error',
            'reached_any_create_before',
            'reached_any_create_after',
            'reached_any_write_before',
            'reached_any_write_partial',
            'reached_any_write_after',
            'reached_any_sync_before',
            'reached_any_sync_after',
            'reached_any_rename_before',
            'reached_any_rename_after',
            'reached_any_directory_sync_before',
            'reached_any_directory_sync_after',
            'reached_any_unlink_before',
            'reached_any_unlink_after',
            'reached_wal_commit_write_before',
            'reached_wal_commit_write_partial',
            'reached_wal_commit_write_after',
            'reached_wal_commit_sync_before',
            'reached_wal_commit_sync_after',
            'reached_recovery_old_file_sync_before',
            'reached_recovery_old_file_sync_after',
            'reached_current_write_before',
            'reached_current_write_partial',
            'reached_current_write_after',
            'reached_current_sync_before',
            'reached_current_sync_after',
            'reached_current_rename_before',
            'reached_current_rename_after',
            'reached_current_directory_sync_before',
            'reached_current_directory_sync_after',
            'reached_writer_write_before',
            'reached_writer_write_partial',
            'reached_writer_write_after',
            'reached_writer_sync_before',
            'reached_writer_sync_after',
            'reached_writer_rename_before',
            'reached_writer_rename_after',
            'reached_writer_directory_sync_before',
            'reached_writer_directory_sync_after',
            'reached_retirement_directory_sync_before',
            'reached_retirement_directory_sync_after',
            'reached_retirement_unlink_before',
            'reached_retirement_unlink_after']},
 {'lane': 'fault_release',
  'strict_count': 137,
  'strict_families': {'boundary': 2,
                      'concatenation': 3,
                      'eof': 1,
                      'identity': 3,
                      'json': 3,
                      'length': 3,
                      'tag': 1,
                      'truncation': 120,
                      'utf8': 1},
  'valid': ['continue', 'return_error']}]
DURABLE_OPERATORS = ['valid_identity', 'byte_flip', 'byte_insert', 'byte_delete', 'length', 'truncate', 'concatenate', 'unknown_field', 'duplicate_field', 'unknown_tag', 'identity_limit', 'invalid_utf8', 'nested_unknown', 'nested_duplicate', 'trailing_json', 'partial_suffix', 'invalid_suffix']

TARGETS = (
    {'target': 'durable_and_fault_codecs_fuzz', 'package': 'xadbtest-protocol',
     'binary': 'durable_fuzz', 'test': 'v3_durable_and_fault_codecs_mutation_fuzz_30_minutes',
     'generator': 'durable-native-four-codecs-mutation-v1', 'seed': 0,
     'smoke': 'durable_and_fault_mutation_corpus_smoke'},
    {'target': 'disk_record_fuzz', 'package': 'xadb-engine', 'binary': 'engine',
     'kind': 'lib', 'test': 'durable::codec_fuzz::disk_record_mutation_v3_30_minutes',
     'generator': 'dev2-disk-record-mutation-v1', 'seed': 0x584144424449534b,
     'smoke': 'durable::codec_fuzz::disk_record_mutation_v2_64_seeds'},
    {'target': 'durable_thread_stress', 'package': 'xadbtest-engine-worker',
     'binary': 'kernel_stress', 'test': 'durable_two_writers_four_readers_v3_30_minutes',
     'generator': 'dev2-durable-thread-v1',
     'smoke': 'durable_two_writers_four_readers_v2_64_seeds'},
    {'target': 'p0_bounded_exhaustive', 'package': 'xadbtest-reference', 'binary': 'bounded',
     'test': 'bounded_two_actor_scalar_programs_exhaustive',
     'generator': 'p0-bounded-net-zero-v2', 'timeout_seconds': 7500,
     'smoke': 'bounded_two_actor_scalar_programs_smoke'},
)


def same(actual, expected):
    if type(actual) is not type(expected):
        return False
    if isinstance(expected, dict):
        return actual.keys() == expected.keys() and all(same(actual[key], value) for key, value in expected.items())
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(same(a, b) for a, b in zip(actual, expected))
    return actual == expected


def equal(record, **expected):
    for key, value in expected.items():
        if not same(record.get(key), value):
            raise ValueError(f'fixed profile/result mismatch: {key}')


def vector(record, field, length, minimum, total, integer):
    values = record.get(field)
    if not isinstance(values, list) or len(values) != length:
        raise ValueError(f'wrong vector inventory: {field}')
    for index, value in enumerate(values):
        integer({field: value}, field, minimum)
    if sum(values) != total:
        raise ValueError(f'wrong conservation: {field}')
    return values


def validate(spec, output, elapsed_seconds, read_receipt, integer, coverage):
    target = spec['target']
    prefix = {'durable_and_fault_codecs_fuzz': 'XADB_V3', 'disk_record_fuzz': 'XADB_DISK_FUZZ',
              'durable_thread_stress': 'XADB_KERNEL_THREAD', 'p0_bounded_exhaustive': 'XADB_BOUNDED'}[target]
    profile = read_receipt(output, prefix + '_PROFILE ')
    result = read_receipt(output, prefix + '_RESULT ')
    equal(profile, generator=spec['generator'])
    equal(result, generator=spec['generator'])
    if 'seed' in spec:
        equal(profile, seed=spec['seed'])
        equal(result, seed=spec['seed'])
    bounded = target == 'p0_bounded_exhaustive'
    measured = integer(result, 'elapsed_ms', 0 if bounded else 1800000)
    if measured > elapsed_seconds * 1000 + 1 or (not bounded and elapsed_seconds < 1800):
        raise ValueError('impossible or short elapsed-time containment')
    if not bounded:
        equal(profile, min_seconds=1800, seed_count=256)
    if target == 'durable_and_fault_codecs_fuzz':
        equal(profile, target=target, operations_per_seed=1024, round_cap=1000000000,
              input_cap=8388612, lanes=DURABLE_LANES, operators=DURABLE_OPERATORS,
              coverage_guided=False, independent_oracle='literal_acceptance_only',
              fault_transport_caps_included=False)
        equal(result, target=target, seed_count=256, operations_per_seed=1024, missing_lanes=0)
        rounds = integer(result, 'rounds', 256 * 1024)
        if rounds >= 1000000000:
            raise ValueError('declared round cap reached')
        equal(result, mutations=4 * rounds)
        lanes = result.get('lane_counts')
        if not isinstance(lanes, dict) or set(lanes) != {item['lane'] for item in DURABLE_LANES}:
            raise ValueError('missing/foreign durable codec lane')
        for literal in DURABLE_LANES:
            lane = lanes[literal['lane']]
            valid = len(literal['valid'])
            cases = valid + literal['strict_count'] + rounds
            equal(lane, cases=cases, mutations=rounds, seed_operator_pairs=256 * 17,
                  seed_operator_min_visits=rounds // (256 * 17))
            if integer(lane, 'accepted', valid) + integer(lane, 'rejected', 1) != cases:
                raise ValueError('durable lane case conservation failed')
            if (integer(lane, 'known_valid', valid) > lane['accepted'] or
                    integer(lane, 'known_invalid', 1) > lane['rejected']):
                raise ValueError('durable lane known cases exceed observed outcomes')
            variants = coverage(lane, 'variant_visits', literal['valid'])
            if any(count != 1 for count in variants.values()):
                raise ValueError('literal variant executed other than once')
            equal(lane, family_visits={**literal['strict_families'], 'valid': valid, 'mutation': rounds})
            operators = coverage(lane, 'operator_visits', DURABLE_OPERATORS)
            if [operators[name] for name in DURABLE_OPERATORS] != [
                    rounds // 17 + int(index < rounds % 17) for index in range(17)]:
                raise ValueError('operator schedule differs from frozen generator')
            equal(lane, known_valid=valid + 3 + operators['valid_identity'] + operators['concatenate'],
                  known_invalid=literal['strict_count'] - 3 + sum(operators[name] for name in DURABLE_OPERATORS[7:]))
            seeds = vector(lane, 'seed_visits', 256, 1024, rounds, integer)
            if seeds != [rounds // 256 + int(index < rounds % 256) for index in range(256)]:
                raise ValueError('seed schedule differs from frozen generator')
    elif target == 'disk_record_fuzz':
        equal(profile, operations_per_seed=1024, input_cap=1048618, coverage_guided=False,
              variants=16, operators=11)
        cases = integer(result, 'cases', 256 * 1024)
        if integer(result, 'accepted', 1) + integer(result, 'rejected', 1) != cases:
            raise ValueError('disk codec case conservation failed')
        seeds = vector(result, 'seed_visits', 256, 1024, cases, integer)
        if cases % 1024 or any(value % 1024 for value in seeds):
            raise ValueError('partial disk seed work')
        rounds = cases // 1024
        if seeds != [1024 * (rounds // 256 + int(index < rounds % 256)) for index in range(256)]:
            raise ValueError('disk seed schedule mismatch')
        variants = vector(result, 'variant_visits', 16, 1, cases, integer)
        operators = vector(result, 'operator_visits', 11, 1, cases, integer)
        if variants != [cases // 16] * 16:
            raise ValueError('disk variant schedule mismatch')
        expected_operators = [sum((visits // 1024) * 16 * (5 + int((operator - seed) % 11 < 9))
                                  for seed, visits in enumerate(seeds)) for operator in range(11)]
        if operators != expected_operators:
            raise ValueError('disk operator schedule mismatch')
    elif target == 'durable_thread_stress':
        equal(profile, writer_attempts_per_seed=1024, readers=4, writers=2, power_loss=False)
        rounds = integer(result, 'rounds', 256)
        equal(result, commits=rounds * 512, conflicts=rounds * 512)
        samples = integer(result, 'sampler_checks', rounds)
        equal(result, reader_row_checks=rounds * 512 * 3 * 4 + 2 * samples)
        for field in ('sampler_overlap_begin', 'sampler_overlap_end', 'sampler_overlap_both'):
            if integer(result, field, 1) > samples:
                raise ValueError('sampler overlap exceeds actual samples')
        if result['sampler_overlap_both'] > min(result['sampler_overlap_begin'], result['sampler_overlap_end']):
            raise ValueError('impossible sampler overlap intersection')
        seeds = vector(result, 'seed_visits', 256, 1, rounds, integer)
        if seeds != [rounds // 256 + int(index < rounds % 256) for index in range(256)]:
            raise ValueError('kernel stress seed schedule mismatch')
    else:
        excluded = 'Insert,Delete,multi-mutation,rollback,UNKNOWN,crash,catalog'
        scope = 'all raw merges classified; checker runs semantic representatives'
        equal(profile, profile='exhaustive', initial_states=256, programs_per_actor=13,
              raw_event_schedules=12870, interval_classes=19, linear_orders=70, critical_orders=6,
              expected_cases=9604608, expected_accepted=7003648, expected_rejected=2600960,
              literal_cases=13, max_seconds=7200, excluded=excluded, schedule_scope=scope,
              grammar='12 BeginRW-Replace-Read-Commit;1 BeginRO-Scan1-Scan2-Commit')
        equal(result, status='PASS', profile='exhaustive', excluded=excluded, schedule_scope=scope,
              raw_event_schedules=12870, interval_classes=19, critical_orders=6, program_pairs=43264,
              serial_cases=3028480, signature_cases=4932096, mutant_cases=1644032,
              scalar_reduction_checks=3028480, expected_cases=9604608, completed_cases=9604608,
              accepted=7003648, rejected=2600960, invalid=0, incomplete=0, literal_cases=13,
              missing_cases=0)
        integer(result, 'max_states', 1)
        integer(result, 'max_transitions', 1)
        if measured >= 7200000:
            raise ValueError('bounded exploration exceeded its declared wall cap')
    return {'profile': profile, 'result': result}
