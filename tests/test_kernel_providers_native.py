"""Exercise removal planning with real RPM queries in a disposable database.

The generated packages have no files or scriptlets. Only their headers are
installed, using --dbpath and --justdb; the host RPM database is never used.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parents[1] / 'data/scripts/kernel-manager'
NEVRA = '%{NAME}-%{EPOCHNUM}:%{VERSION}-%{RELEASE}.%{ARCH}\n'
VERSIONS = ('7.2.10-200.nobara.fc44.x86_64', '7.2.11-200.nobara.fc44.x86_64')


@unittest.skipUnless(shutil.which('rpm') and shutil.which('rpmbuild'), 'RPM build tools unavailable')
class NativeKernelProviderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='nobara-kernel-providers-')
        cls.addClassCleanup(cls.temp.cleanup)
        cls.root = Path(cls.temp.name)
        cls.rpm = shutil.which('rpm')
        cls.kernels = [cls.build('kernel-core', str(i + 1), provides='kernel-uname-r = ' + version)
                       for i, version in enumerate(VERSIONS)]
        cls.modules = [cls.build('kmod-fixture-' + version, '1', requires='kernel-uname-r = ' + version)
                       for version in VERSIONS]
        cls.shared = cls.build('custom-boot-kernel', '1', provides='kernel-uname-r = ' + VERSIONS[0])
        cls.unrelated = cls.build('custom-other-kernel', '1', provides='kernel-uname-r = 5.0-1.x86_64')

    @classmethod
    def build(cls, name, version, *, provides='', requires=''):
        spec = cls.root / f'{name}-{version}.spec'
        spec.write_text(f'''Name: {name}
Version: {version}
Release: 1
Summary: Disposable kernel provider test
License: MIT
BuildArch: noarch
{('Provides: ' + provides) if provides else ''}
{('Requires: ' + requires) if requires else ''}
%description
Header-only package for testing kernel ABI queries.
%files
''')
        result = subprocess.run(['rpmbuild', '--define', f'_topdir {cls.root / "build"}',
                                 '--define', f'_tmppath {cls.root}', '-bb', str(spec)],
                                text=True, capture_output=True)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return cls.root / 'build/RPMS/noarch' / f'{name}-{version}-1.noarch.rpm'

    def plan(self, extra=()):
        with tempfile.TemporaryDirectory(dir=self.root) as directory:
            case = Path(directory)
            db = case / 'rpmdb'
            subprocess.run([self.rpm, '--dbpath', str(db), '--initdb'], check=True, capture_output=True)
            subprocess.run([self.rpm, '--dbpath', str(db), '--justdb', '--nodeps', '--noscripts',
                            '--notriggers', '--noplugins', '-i',
                            *map(str, [*self.kernels, *self.modules, *extra])], check=True, capture_output=True)
            # Source only the function definitions, with no live switch or
            # rescue dispatch. sudo can only execute queries in our test DB.
            script = case / 'plan'
            definitions = SOURCE.read_text().split('[[ $# -eq 2 ]] || {', 1)[0]
            script.write_text(definitions + '''
disabled_packages=()
switch_removals=()
plan_kernel_removals
printf '%s\n' "${switch_removals[@]}"
''')
            sudo = case / 'sudo'
            sudo.write_text('''#!/usr/bin/env python3
import os, sys
args = sys.argv[1:]
if not args or args[0] != 'rpm' or args[1] not in ('-q', '-qa'):
    raise SystemExit('Only isolated RPM queries are allowed')
os.execv(os.environ['TEST_RPM'], [os.environ['TEST_RPM'], '--dbpath', os.environ['TEST_RPM_DB'], *args[1:]])
''')
            sudo.chmod(0o755)
            inventory_command = [self.rpm, '--dbpath', str(db), '-qa', '--qf', NEVRA]
            before = subprocess.check_output(inventory_command, text=True)
            result = subprocess.run(['bash', str(script)], text=True, capture_output=True,
                                    env=dict(os.environ, PATH=str(case) + ':' + os.environ['PATH'],
                                             TEST_RPM=self.rpm, TEST_RPM_DB=str(db)))
            self.assertEqual(subprocess.check_output(inventory_command, text=True), before)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return set(result.stdout.splitlines())

    def test_multiple_installed_kernels_and_exact_generated_modules(self):
        removed = self.plan([self.unrelated])
        self.assertEqual(removed, {'kernel-core-0:1-1.noarch', 'kernel-core-0:2-1.noarch',
                                   *(f'kmod-fixture-{v}-0:1-1.noarch' for v in VERSIONS)})

    def test_retained_provider_protects_only_modules_for_its_exact_abi(self):
        removed = self.plan([self.shared, self.unrelated])
        self.assertEqual(removed, {'kernel-core-0:1-1.noarch', 'kernel-core-0:2-1.noarch',
                                   f'kmod-fixture-{VERSIONS[1]}-0:1-1.noarch'})


if __name__ == '__main__':
    unittest.main()
