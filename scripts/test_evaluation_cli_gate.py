"""Обязательный гейт действительно вызывает SCI и внешний CLI без ручного флага."""
from contextlib import redirect_stdout
import io
import unittest
from unittest.mock import patch
import authority_mutation_gate as gate


class CliGateWiring(unittest.TestCase):
    def test_required_gate_calls_both_science_and_cli_probes(self):
        # Предметная проверка исполняется отдельно; здесь проверяется реальный
        # маршрут main с подменёнными дорогими эффектами, не текстовый маркер.
        with patch.object(gate, "run_mutant"), patch.object(gate, "verify_evaluation_borrows"), \
             patch.object(gate.subprocess, "run") as calls, redirect_stdout(io.StringIO()):
            gate.main()
        scripts = [call.args[0][1] for call in calls.call_args_list]
        self.assertEqual(scripts, [str(gate.ROOT / "scripts/science_certificate_gate.py"),
                                   str(gate.ROOT / "scripts/evaluation_cli_gate.py")])
        for call in calls.call_args_list:
            self.assertIs(call.kwargs["check"], True)
            self.assertEqual(call.kwargs["cwd"], gate.ROOT)


if __name__ == "__main__":
    unittest.main()
