"""Frozen known-bad mutations applied to already-validated observed receipts."""
from copy import deepcopy

from dev3_storage_model import check_trace, check_images


def reject(call, family, key):
    try:
        call()
    except ValueError as error:
        if str(error) == family:
            return {'case': key, 'status': 'EXPECTED_REJECTION', 'failure': family}
        raise ValueError('FAIL_CHECKER_WRONG_REJECTION: ' + key) from error
    raise ValueError('FAIL_CHECKER_ACCEPTED_BAD_INPUT: ' + key)


def trace_mutations(actual, expected, cases):
    check_trace(actual, expected)
    result = []
    for key in cases:
        value, error = deepcopy(actual), 'FAIL_EFFECT_ORDER'
        if key == 'missing_event':
            value.pop(0)
            error = 'FAIL_EFFECT_CENSUS'
        elif key == 'duplicate_event':
            value.insert(0, deepcopy(value[0]))
            error = 'FAIL_EFFECT_CENSUS'
        elif key == 'wrong_order':
            value[0], value[1] = value[1], value[0]
        elif key == 'wrong_name':
            value[1]['name'] = 'other'
        elif key == 'wrong_offset':
            value[1]['offset'] += 1
        elif key == 'wrong_length':
            value[1]['length'] -= 1
        elif key == 'wrong_digest':
            value[1]['sha256'] = '0' * 64
            error = 'FAIL_EFFECT_BYTES'
        else:
            raise ValueError('FAIL_FROZEN_MUTANT')
        result.append(reject(lambda: check_trace(value, expected), error, key))
    return result


def image_mutations(actual, expected, cases):
    check_images(actual, expected)
    result = []
    for key in cases:
        value, error = deepcopy(actual), 'FAIL_DURABLE_IMAGE'
        if key == 'missing_cut':
            value.pop(0)
            error = 'FAIL_CUT_CENSUS'
        elif key == 'duplicate_cut':
            value.insert(0, deepcopy(value[0]))
            error = 'FAIL_CUT_CENSUS'
        elif key == 'missing_image':
            value[1]['images'].pop()
        elif key == 'extra_image':
            value[1]['images'].append(deepcopy(value[1]['images'][0]))
        elif key == 'wrong_name':
            value[-1]['images'][0][0]['name'] = 'other'
        elif key == 'wrong_length':
            value[-1]['images'][0][0]['length'] += 1
        elif key == 'wrong_digest':
            value[-1]['images'][0][0]['sha256'] = '0' * 64
        else:
            raise ValueError('FAIL_FROZEN_MUTANT')
        result.append(reject(lambda: check_images(value, expected), error, key))
    return result


def observation_mutations(actual, expected, cases, checker):
    checker(actual, expected)
    result = []
    for key in cases:
        value, error = deepcopy(actual), 'FAIL_OBSERVATION_ORDER'
        if key == 'missing_observation':
            value.pop(0)
            error = 'FAIL_OBSERVATION_CENSUS'
        elif key == 'wrong_phase':
            value[0]['phase'] = 'readback'
        elif key == 'wrong_name':
            value[0]['name'] = 'other'
        elif key == 'wrong_offset':
            value[-2]['offset'] += 1
        elif key == 'wrong_length':
            value[-2]['length'] += 1
        else:
            raise ValueError('FAIL_FROZEN_MUTANT')
        result.append(reject(lambda: checker(value, expected), error, key))
    return result


def native_mutations(actual, expected, actual_keys, required, cases, checker, census):
    checker(actual, expected)
    census(actual_keys, required)
    result = []
    for key in cases:
        value, keys = deepcopy(actual), list(actual_keys)
        if key == 'wrong_source':
            value['source_sha256'], error = '0' * 64, 'FAIL_SOURCE_IDENTITY'
        elif key == 'wrong_binary':
            value['binary_sha256'], error = '0' * 64, 'FAIL_BINARY_IDENTITY'
        elif key == 'missing_child':
            keys.pop()
            error = 'FAIL_NATIVE_CENSUS'
        elif key == 'duplicate_child':
            keys.append(keys[0])
            error = 'FAIL_NATIVE_CENSUS'
        elif key == 'wrong_hook_index':
            value['hook'] += 1
            error = 'FAIL_HOOK'
        elif key == 'wrong_side':
            value['after'], error = not value['after'], 'FAIL_HOOK'
        elif key == 'missing_hook':
            value.update(markers=[], hook_count=0)
            error = 'FAIL_HOOK'
        elif key == 'missed_deadline':
            value['hook_elapsed_ms'], error = 60001, 'FAIL_TIMEOUT'
        elif key in ('wrong_file_hash', 'wrong_inode', 'wrong_length'):
            field = {'wrong_file_hash': 'sha256', 'wrong_inode': 'inode', 'wrong_length': 'length'}[key]
            value['files'][0][field] = '0' * 64 if field == 'sha256' else value['files'][0][field] + 1
            error = 'FAIL_EXTERNAL_IMAGE'
        elif key == 'old_files_changed':
            value['before'][0]['inode'] += 1
            error = 'FAIL_OLD_FILES'
        else:
            raise ValueError('FAIL_FROZEN_MUTANT')
        def run():
            census(keys, required)
            checker(value, expected)
        result.append(reject(run, error, key))
    return result
