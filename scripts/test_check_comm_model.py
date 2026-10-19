import unittest
from check_comm_model import TEST, WITNESSES, check_model


class ModelCheckerTests(unittest.TestCase):
    def test_requires_all_layers_consistent_counts_and_each_minimal_witness(self):
        rows = [f'test {TEST} ... ok']
        rows += [f'COMM_MODEL_LAYER depth={n} states=1 total_states={n+1} transitions={n} elapsed_ms={n}' for n in range(1, 9)]
        rows += [f'COMM_MODEL_WITNESS {name} [Event ...]' for name in sorted(WITNESSES)]
        rows += ['COMM_MODEL_COMPLETE depth=8 states=9 transitions=8 witnesses=7']
        valid = '\n'.join(rows)
        self.assertEqual(check_model(valid, 0)['states'], 9)
        for bad, code in [(valid, 101), (valid + '\nINCOMPLETE', 0),
                          (valid.replace('depth=8', 'depth=7'), 0),
                          (valid.replace('total_states=5', 'total_states=6'), 0),
                          (valid.replace('transitions=8 witnesses', 'transitions=9 witnesses'), 0),
                          (valid.replace('partial_tx_cancel', 'partial_rx_cancel'), 0),
                          (valid.replace('elapsed_ms=8', 'elapsed_ms=21600000'), 0)]:
            with self.assertRaises(ValueError):
                check_model(bad, code)


if __name__ == '__main__':
    unittest.main()
