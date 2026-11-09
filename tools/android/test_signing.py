import base64
from pathlib import Path
import tempfile
import unittest
from signing import configure, escape_property


class SigningTests(unittest.TestCase):
    def test_requires_existing_credentials_and_preserves_files(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            for encoded, password in [('', 'x'), ('YWJj', ''), ('not base64', 'x')]:
                with self.assertRaises(ValueError):
                    configure(root, encoded, password)
                self.assertEqual(list(root.iterdir()), [])
            configure(root, base64.b64encode(b'fixture only').decode(), ' test\\password\nю')
            self.assertEqual((root / 'kouros-android.jks').read_bytes(), b'fixture only')
            self.assertEqual((root / 'kouros-android.jks').stat().st_mode & 0o777, 0o600)
            self.assertIn('password=\\ test\\\\password\\nю\n', (root / 'keystore.properties').read_text())
            with self.assertRaises(FileExistsError):
                configure(root, 'YWJj', 'replacement')
            self.assertEqual((root / 'kouros-android.jks').read_bytes(), b'fixture only')

    def test_property_values_cannot_inject_new_entries(self):
        value = escape_property('x\nstoreFile=other\r\t\\')
        self.assertNotIn('\n', value)
        self.assertNotIn('\r', value)
        self.assertTrue(value.endswith('\\\\'))


if __name__ == '__main__':
    unittest.main()
