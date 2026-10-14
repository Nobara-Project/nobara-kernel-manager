"""Run the real switch script against disposable repos and simulated RPM/DNF.

No privileges, installed packages, boot files, or network access are used.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[1] / 'data/scripts/kernel-manager'
MANAGED = ('kernel-modules', 'kernel-core', 'kernel-devel', 'kernel-devel-matched', 'kernel-headers', 'kernel')

FAKE = r'''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
name, args = Path(sys.argv[0]).name, sys.argv[1:]
if name == 'sudo':
    if args == ['-v']: sys.exit(0)
    os.execvp(args[0], args)
p = Path(os.environ['SWITCH_FIXTURE'])
s = json.loads(p.read_text())
with p.with_suffix('.log').open('a') as f: f.write(json.dumps([name, *args]) + '\n')
def done(output='', status=0):
    if output: print(output)
    sys.exit(status)
def selected(specs):
    return [v for v in s['packages'] if v['name'] in specs or v['nevra'] in specs]
def save(): p.write_text(json.dumps(s))
if name == 'rpm':
    if '-qa' in args:
        if s.get('fail') == 'inventory': done(status=1)
        done('\n'.join(v['name'] + '\t' + v['nevra'] for v in s['packages']))
    if '--whatprovides' in args:
        if s.get('fail') == 'providers': done(status=1)
        dep = args[args.index('--whatprovides') + 1]
        found = [v for v in s['packages'] if dep in v['provides']]
        done('\n'.join(v['nevra'] for v in found), 0 if found else 1)
    if '--provides' in args or '--requires' in args:
        field = 'provides' if '--provides' in args else 'requires'
        if s.get('fail') == field: done(status=1)
        found = selected(args[args.index('--' + field) + 1:])
        done('\n'.join(dep for v in found for dep in v[field]), 0 if found else 1)
    if '-e' in args:
        removed = args[args.index('--nodeps') + 1:]
        s['packages'] = [v for v in s['packages'] if v['nevra'] not in removed]
        save()
        done()
    if '-q' in args:
        found = selected(args)
        done('\n'.join(v['nevra'] for v in found), 0 if found else 1)
if name == 'dnf5':
    if 'makecache' in args: done(status=1 if s.get('fail') == 'metadata' else 0)
    if 'repoquery' in args:
        if '--installed' in args:
            repo = next(a.split('=', 1)[1] for a in args if a.startswith('--installed-from-repo='))
            done('\n'.join(v['nevra'] for v in s['packages'] if v['origin'] == repo))
        if s.get('fail') == 'available': done(status=1)
        done(args[-1] + '-0:9-1.noarch')
    if 'install' in args or 'upgrade' in args:
        pkg = args[-1]
        s['packages'].append(dict(name=pkg, nevra=pkg + '-0:9-1.noarch', provides=[], requires=[], origin='selected'))
        save()
        done()
if name in ('dracut', 'update-grub'): done()
done('Unexpected fake command: ' + repr([name, *args]), 99)
'''


def package(name, *, version='1', origin='nobara-kernel-lts', provides=(), requires=()):
    return dict(name=name, nevra=f'{name}-0:{version}-1.noarch', origin=origin,
                provides=list(provides), requires=list(requires))


class KernelSwitchTests(unittest.TestCase):
    def run_switch(self, *, target='mainline', extra=(), fail=None):
        old_repo = 'nobara-kernel-' + ('lts' if target == 'mainline' else 'mainline')
        kernel = '6.18.52-201.lts.nobara.fc44.noarch' if target == 'mainline' else '7.2.4-201.nobara.fc44.noarch'
        dependency = 'kernel-uname-r = ' + kernel
        old = [package(name, origin=old_repo, provides=[dependency] if name == 'kernel-core' else []) for name in MANAGED]
        modules = [package(f'kmod-{driver}-{kernel}', origin='@commandline', requires=[dependency])
                   for driver in ('wl', 'v4l2loopback')]
        # Preserve source/metapackages even with misleading repository origins.
        sources = [package(name, origin=old_repo) for name in ('akmod-wl', 'broadcom-wl', 'kmod-wl', 'akmods')]
        other = package('kmod-wl-5.0-1.noarch', origin='@commandline', requires=['kernel-uname-r = 5.0-1.noarch'])
        with tempfile.TemporaryDirectory(prefix='nobara-kernel-switch-') as d:
            root = Path(d)
            repos = root / 'repos'
            disabled = repos / 'no-touch-disabled'
            disabled.mkdir(parents=True)
            for family in ('mainline', 'lts'):
                enabled = family != target
                path = (repos if enabled else disabled) / f'nobara-kernel-{family}.repo'
                path.write_text(f'[nobara-kernel-{family}]\nname=Fixture\nenabled={int(enabled)}\n')
            script = root / 'kernel-manager'
            script.write_text(SOURCE.read_text().replace('readonly REPO_DIR="/etc/yum.repos.d"', f'readonly REPO_DIR="{repos}"'))
            bins = root / 'bin'
            bins.mkdir()
            for command in ('sudo', 'rpm', 'dnf5', 'dracut', 'update-grub'):
                path = bins / command
                path.write_text(FAKE)
                path.chmod(0o755)
            state = root / 'state.json'
            state.write_text(json.dumps(dict(packages=old + modules + sources + [other] + list(extra), fail=fail)))
            result = subprocess.run(['bash', str(script), 'switch', target], capture_output=True, text=True,
                                    env=dict(os.environ, PATH=str(bins) + ':' + os.environ['PATH'], SWITCH_FIXTURE=str(state)))
            calls = [json.loads(line) for line in state.with_suffix('.log').read_text().splitlines()]
            final = json.loads(state.read_text())['packages']
            selected_repo = (repos / f'nobara-kernel-{target}.repo').read_text()
            return result, calls, final, old + modules, sources + [other], selected_repo

    def test_both_switch_directions_remove_matching_modules_with_kernels(self):
        for target in ('mainline', 'lts'):
            with self.subTest(target=target):
                result, calls, final, retired, kept, repo = self.run_switch(target=target)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                erases = [c for c in calls if c[:2] == ['rpm', '-e']]
                self.assertEqual(len(erases), 1)
                self.assertEqual(set(erases[0][3:]), {v['nevra'] for v in retired})
                self.assertTrue({v['nevra'] for v in kept} <= {v['nevra'] for v in final})
                self.assertIn('enabled=1', repo)
                self.assertEqual(len([v for v in final if v['name'] in MANAGED]), len(MANAGED))
                erase_index = calls.index(erases[0])
                self.assertTrue(any('makecache' in c for c in calls[:erase_index]))

    def test_shared_kernel_provider_keeps_its_modules(self):
        kernel = '6.18.52-201.lts.nobara.fc44.noarch'
        extra = package('custom-boot-kernel', origin='@commandline', provides=['kernel-uname-r = ' + kernel])
        result, calls, final, retired, _, _ = self.run_switch(extra=[extra])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        retained = {v['nevra'] for v in final}
        self.assertTrue({v['nevra'] for v in retired if v['name'].startswith('kmod-')} <= retained)
        self.assertIn(extra['nevra'], retained)

    def test_matching_name_without_matching_kernel_requirement_is_preserved(self):
        name = 'kmod-custom-6.18.52-201.lts.nobara.fc44.noarch'
        extra = package(name, origin='@commandline', requires=['custom-abi'])
        result, _, final, _, _, _ = self.run_switch(extra=[extra])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(extra['nevra'], {v['nevra'] for v in final})

    def test_query_or_repository_failure_stops_before_removal(self):
        for fail in ('inventory', 'provides', 'providers', 'requires', 'metadata', 'available'):
            with self.subTest(fail=fail):
                result, calls, final, retired, _, _ = self.run_switch(fail=fail)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(c[:2] == ['rpm', '-e'] for c in calls))
                self.assertTrue({v['nevra'] for v in retired} <= {v['nevra'] for v in final})


if __name__ == '__main__':
    unittest.main()
