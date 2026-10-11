from __future__ import annotations

lazy import ast
lazy import hashlib
lazy import tempfile
lazy import unittest
lazy from pathlib import Path
lazy from unittest.mock import patch
lazy from project_tools.prepare_semgrep_sources import c_view, original_column, prepare, python_view, remap_sarif
lazy import json


class ParserViewTests(unittest.TestCase):
    def test_lazy_aliases_relative_multiline_and_conditional_imports(self):
        source = ('lazy import json as j; lazy import os\n'
                  'lazy from .helpers import (\n    load as read,\n)\n'
                  'if True:\n    lazy from pathlib import Path\n'
                  'value = "lazy import untouched" # lazy from comment\n')
        result = python_view(source)
        self.assertEqual(source.count("\n"), result.count("\n"))
        self.assertEqual(len(source), len(result))
        self.assertIn('"lazy import untouched" # lazy from comment', result)
        before = ast.parse(source)
        after = ast.parse(result)
        before_imports = [node for node in ast.walk(before) if isinstance(node, (ast.Import, ast.ImportFrom))]
        after_imports = [node for node in ast.walk(after) if isinstance(node, (ast.Import, ast.ImportFrom))]
        for original, converted in zip(before_imports, after_imports, strict=True):
            self.assertTrue(original.is_lazy)
            self.assertFalse(converted.is_lazy)
            original.is_lazy = 0
            self.assertEqual(ast.dump(original), ast.dump(converted))
            self.assertEqual(original.lineno, converted.lineno)
            self.assertEqual(original.names[0].col_offset, converted.names[0].col_offset)

    def test_invalid_python_is_not_silently_normalized(self):
        for source in ('lazy import json\nvalue = (\n',
                       'def f():\n    lazy import json\n',
                       'class C:\n    lazy import json\n',
                       'try:\n    lazy import json\nexcept ImportError:\n    pass\n',
                       'lazy from os import *\n',
                       'lazy from __future__ import annotations\n'):
            with self.subTest(source=source), self.assertRaises(SyntaxError):
                python_view(source)

    def test_c_macro_expansion_retains_literals_comments_branches_and_security_calls(self):
        source = ('// IPSTR m3ApiGetArg(int, untouched)\n'
                  'const char *label = "PRIu32 IPSTR";\n'
                  '#if FEATURE\nstrcpy(a, b);\n#else\nstrcpy(c, d);\n#endif\n'
                  'static m3ApiRawFunction(host_read)\n{\n'
                  'm3ApiReturnType(float);\nm3ApiGetArg(int32_t, index);\n'
                  'm3ApiReturn(data[index]);\n}\n'
                  'printf("ip=" IPSTR " count=%" PRIu32, ip, count);\n')
        converted, edits = c_view(source)
        self.assertEqual(source.count("\n"), converted.count("\n"))
        self.assertIn('// IPSTR m3ApiGetArg(int, untouched)', converted)
        self.assertIn('"PRIu32 IPSTR"', converted)
        self.assertIn('#if FEATURE\nstrcpy(a, b);\n#else\nstrcpy(c, d);\n#endif', converted)
        self.assertIn('const void * host_read (IM3Runtime runtime, IM3ImportContext _ctx, uint64_t * _sp, void * _mem)', converted)
        self.assertIn('int32_t  index = *((int32_t *) (_sp++))', converted)
        self.assertIn('*raw_return = (data[index]); return m3Err_none;', converted)
        self.assertEqual(len(edits), 6)
        line = converted.splitlines()[-1]
        original_line = source.splitlines()[-1]
        self.assertEqual(original_column(line.index('count);') + 1, len(source.splitlines()), edits), original_line.index('count);') + 1)

    def test_unknown_or_multiline_macro_form_fails_closed(self):
        for source in ('m3ApiGetArg(\nint32_t, index)', 'm3ApiRawFunction\n(host)',
                       'm3ApiReturnType\r\n(float)'):
            with self.subTest(source=source), self.assertRaises(ValueError):
                c_view(source)
        source = 'UNKNOWN_MACRO(type, input);'
        self.assertEqual(c_view(source), (source, []))

    def test_sarif_and_code_flow_columns_resolve_to_the_original_file(self):
        root = Path(__file__).resolve().parents[2]
        name = 'software/ruview/firmware/esp32-mmwave-node/main/measurement_stream.c'
        source = (root / name).read_text()
        converted, edits = c_view(source)
        edit = edits[0]
        original_line = source.splitlines()[edit['line'] - 1]
        converted_line = converted.splitlines()[edit['line'] - 1]
        physical = {'artifactLocation': {'uri': name}, 'region': {
            'startLine': edit['line'], 'startColumn': len(converted_line) + 1,
        }}
        with tempfile.TemporaryDirectory() as temporary:
            sarif = Path(temporary) / 'test.sarif'
            mapping = Path(temporary) / 'mapping.json'
            sarif.write_text(json.dumps({'runs': [{'results': [{'locations': [{'physicalLocation': physical}],
                'codeFlows': [{'threadFlows': [{'locations': [{'location': {'physicalLocation': physical}}]}]}]}]}]}))
            mapping.write_text(json.dumps({name: edits}))
            remap_sarif(sarif, mapping)
            result = json.loads(sarif.read_text())['runs'][0]['results'][0]
            self.assertEqual(result['locations'][0]['physicalLocation']['region']['startColumn'], len(original_line) + 1)
            self.assertEqual(result['codeFlows'][0]['threadFlows'][0]['locations'][0]['location']['physicalLocation']['region']['startColumn'], len(original_line) + 1)

    def test_public_view_preserves_originals_and_directory_not_filename_exclusions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'source'
            root.mkdir()
            names = ['project_tools/check.py', 'software/ruview/docs/build-guide.md',
                     'software/ruview/build-cache/secret.py', 'software/ruview/private/secret.py',
                     'software/ruview/vendor/dependency.py']
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('lazy import json\n')
            before = {name: hashlib.sha256((root / name).read_bytes()).digest() for name in names}
            with patch('project_tools.prepare_semgrep_sources.subprocess.check_output', return_value='\0'.join(names).encode()):
                output = Path(temporary) / 'view'
                prepare(root, output)
            self.assertTrue((output / names[0]).is_file())
            self.assertTrue((output / names[1]).is_file())
            self.assertFalse(any((output / name).exists() for name in names[2:]))
            self.assertEqual(before, {name: hashlib.sha256((root / name).read_bytes()).digest() for name in names})
            with self.assertRaises(FileExistsError):
                prepare(root, output)


if __name__ == '__main__':
    unittest.main()
