#!/usr/bin/env python3
# Simulates Greaseweazle output (formats copied from gw sources) for GUI testing.
import sys, time, random
sys.stdout = sys.stderr
a = sys.argv[1:]
if a and a[0] == '--time': a = a[1:]
cmd = a[0] if a else ''
if __import__('os').environ.get('FAKE_LOG'):
    open(__import__('os').environ['FAKE_LOG'], 'a').write(' '.join(a) + '\n')
if cmd in ('', '--help', '-h'):
    print('Usage: gw [--time] [action] [-h] ...\nActions:\n  info  read  write  convert  erase  rpm'); sys.exit(0)
if cmd == 'read' and '--help' in a:
    sys.exit(1)  # the GUI then uses its built-in format list
if cmd == 'info':
    print("Host Tools: 1.23\nDevice:\n  Port:     /dev/ttyACM0\n  Model:    Greaseweazle V4.1\n  MCU:      AT32F403A, 216MHz, 224kB SRAM\n  Firmware: 1.5\n  Serial:   GW0123456789\n  USB:      Full Speed (12 Mbit/s), 128kB Buffer\n\n*** New firmware version 1.6 is available"); sys.exit(0)
if cmd == 'rpm':
    for i in range(int(a[a.index('--nr')+1])):
        r = 300 + random.uniform(-1.2, 1.2); print("Rate: %.3f rpm ; Period: %.3f ms" % (r, 60000/r)); time.sleep(0.05)
    print("***\nFASTEST:  Rate: 301.100 rpm ; Period: 199.269 ms\nAr.Mean:  Rate: 300.050 rpm ; Period: 199.967 ms\nMedian:   Rate: 300.010 rpm ; Period: 199.993 ms\nSLOWEST:  Rate: 298.900 rpm ; Period: 200.736 ms"); sys.exit(0)
if cmd == 'read' and __import__('os').environ.get('FAKE_DISK') == 'erased':
    # A disk erased by gw: only track 0 was partly rewritten (real capture pattern).
    sp = next((x for x in a if x.startswith('c=')), 'c=0-79:h=0-1')
    print("Reading %s revs=1.1\nFormat amiga.amigados" % sp)
    for c in range(80) if sp.startswith('c=0-79') else range(2, 11):
        for h in range(2):
            ok = {(0, 0): 10, (0, 1): 8}.get((c, h), 0)
            print("T%d.%d: AmigaDOS (%d/11 sectors) from Raw Flux (40000 flux in 219.05ms)" % (c, h, ok))
            print("T%d.%d: Giving up: %d sectors missing" % (c, h, 11 - ok))
            time.sleep(0.003)
    sys.exit(0)
def parse_set(x):
    out = []
    for part in x.split(','):
        lo, _, hi = part.partition('-')
        out += list(range(int(lo), int(hi or lo) + 1))
    return out
if cmd == 'read':
    import os
    spec = a[a.index('--tracks') + 1] if '--tracks' in a else 'c=0-79:h=0-1'
    cyls, heads = list(range(80)), [0, 1]
    for part in spec.split(':'):
        if part.startswith('c='): cyls = parse_set(part[2:])
        if part.startswith('h='): heads = parse_set(part[2:])
    retries = int(a[a.index('--retries') + 1]) if '--retries' in a else 3
    print("Reading c=%s:h=%s revs=3\nFormat amiga.amigados" % (spec.split(':')[0][2:] if spec.startswith('c=') else '0-79', '0-1' if heads == [0, 1] else heads[0]))
    random.seed(4); stop = int(os.environ.get('FAKE_STOP', '160'))
    n = 0; summary = {}
    for c in cyls:
        for h in heads:
            n += 1
            if n > stop: time.sleep(3600)
            t = "T%d.%d" % (c, h)
            bad = (c in (17, 18) and h == 1) or (c == 44 and h == 0)
            # Harder retries (Recovery profile) bring back two of the three bad tracks.
            recovered = bad and retries >= 8 and (c, h) != (18, 1)
            weak = random.random() < 0.05
            if bad and not recovered:
                for r in range(1, min(retries, 3) + 2):
                    print("%s: AmigaDOS (%d/11 sectors) from Raw Flux (100000 flux in 600.00ms)%s" % (t, 8, "" if r == 1 else " (Retry #0.%d)" % (r-1)))
                print("%s: Giving up: 3 sectors missing" % t); summary[(c,h)] = [1,1,1,1,0,0,0,1,1,1,1]
            elif recovered or weak:
                print("%s: AmigaDOS (9/11 sectors) from Raw Flux (100000 flux in 600.00ms)" % t)
                print("%s: AmigaDOS (11/11 sectors) from Raw Flux (100000 flux in 600.00ms) (Retry #0.%d)" % (t, 4 if recovered else 1)); summary[(c,h)] = [1]*11
            else:
                print("%s: AmigaDOS (11/11 sectors) from Raw Flux (100000 flux in 600.00ms)" % t); summary[(c,h)] = [1]*11
            time.sleep(0.004 if len(cyls) > 10 else 0.15)
    print("Cyl-> " + "".join(str(c//10) if (i == 0 or c % 10 == 0) else " " for i, c in enumerate(cyls)))
    print("H. S: " + "".join(str(c%10) for c in cyls))
    good = tot = 0
    for h in heads:
        for sct in range(11):
            row = ""
            for c in cyls:
                v = summary[(c,h)][sct]; tot += 1; good += v; row += "." if v else "X"
            print("%d.%2d: %s" % (h, sct, row))
    print("Found %d sectors of %d (%d%%)" % (good, tot, good*100/tot))
    out = a[-1]  # like gw, create the image (an ADF has a fixed size)
    if not os.path.exists(out) or '--no-clobber' not in a:
        src = os.environ.get('FAKE_ADF')
        if src and out.lower().endswith('.adf'):
            # FAKE_ADF: the disk in the drive. Tracks not read stay blank, like gw.
            data = open(src, 'rb').read(); img = bytearray(len(data))
            for c in cyls:
                for h in heads:
                    o = (c * 2 + h) * 11 * 512; img[o:o + 5632] = data[o:o + 5632]
            open(out, 'wb').write(bytes(img))
        else:
            open(out, 'wb').write(bytes(901120 if out.lower().endswith('.adf') else 4096))
    sys.exit(0)

def tracks(ds):
    return [(c, h) for c in range(80) for h in range(2)]
if cmd == 'erase' and __import__('os').environ.get('FAKE_NOINDEX'):
    print("Erasing c=0-81:h=0-1, revs=1\nCommand Failed: GetFluxStatus: No Index"); sys.exit(0)
if cmd == 'write' and __import__('os').environ.get('FAKE_VERIFY_FAIL'):
    # Real capture (Greaseweazle V4.1, worn drive).
    print("Format amiga.amigados\nWriting c=0-79:h=0-1")
    print("T0.0: Writing Track (Flux: 200.0ms period, 219.9 ms total, Terminate at index)")
    print("T0.1: Writing Track (Flux: 200.0ms period, 219.9 ms total, Terminate at index)")
    for r in (1, 2, 3):
        print("T0.1: Writing Track (Verify Failure: Retry #%d)" % r); time.sleep(0.2)
    print("** FATAL ERROR:\nFailed to verify Track 0.1"); sys.exit(1)
if cmd == 'write':
    fmt = a[a.index('--format') + 1] if '--format' in a else 'amiga.amigados'
    geo = {'ibm.360': (40, 2), 'atarist.360': (80, 1), 'msx.1dd': (80, 1), 'commodore.1541': (40, 1),
           'commodore.1571': (35, 2), 'apple2.appledos.140': (35, 1), 'apple2.prodos.140': (35, 1),
           'acorn.dfs.ss': (40, 1), 'acorn.dfs.ss80': (80, 1), 'acorn.dfs.ds': (40, 2)}.get(fmt, (80, 2))
    print("Format %s\nWriting c=0-%d:h=0-%d" % (fmt, geo[0] - 1, geo[1] - 1))
    for c, h in [(c, h) for c in range(geo[0]) for h in range(geo[1])]:
        t = "T%d.%d" % (c, h)
        print("%s: Writing Track (Flux: 200.0ms period, 200.0 ms total, Write all)" % t)
        if (c, h) == (30, 1):
            print("%s: Writing Track (Verify Failure: Retry #1)" % t)
        time.sleep(0.01)
    print("All tracks verified"); sys.exit(0)
if cmd == 'erase':
    print("Erasing c=0-79:h=0-1, revs=1")
    for c, h in tracks(0):
        print("T%d.%d: Erasing Track" % (c, h)); time.sleep(0.005)
    sys.exit(0)
if cmd == 'align':
    n = int(a[a.index('--reads')+1]) if '--reads' in a else 10
    print("Aligning T0.0, reading %d times, revs=3\nFormat amiga.amigados" % n)
    for i in range(n):
        ok = max(0, min(11, int(6 + 5 * (i / max(1, n-1))) + random.randint(-1, 1)))
        print("T0.0: AmigaDOS (%d/11 sectors) from Raw Flux (100000 flux in 600.00ms)" % ok); time.sleep(0.03)
    sys.exit(0)
if cmd == 'delays':
    print("Select Delay: 10us\nStep Delay:   5000us\nSettle Time:  15ms\nMotor Delay:  750ms\nWatchdog:     10000ms\nPre-Write:    100us\nPost-Write:   1000us\nIndex Mask:   200us"); sys.exit(0)
if cmd == 'bandwidth':
    print("\n                   Min.   /   Mean   /   Max.\nWrite Bandwidth:    9.512 /   10.120 /   10.550 Mbps\nRead Bandwidth:    11.004 /   11.230 /   11.600 Mbps\n\nEstimated Consistent Min. Bandwidth: 9.512 Mbps\n -> Max. Flux Rate: 1.189 Msamples/sec\n -> Min. Ave. Flux: 0.841 us"); sys.exit(0)
if cmd == 'pin':
    if a[1] == 'get': print("Pin %s is High" % a[-1])
    else: print("Pin %s is set %s" % (a[-2], "High" if a[-1] == 'H' else "Low"))
    sys.exit(0)
if cmd in ('reset', 'seek'):
    sys.exit(0)
if cmd == 'clean':
    for p in range(1, 3):
        print("Pass %d: " % p, end='', flush=True)
        for c in range(0, 80, 8):
            print("%d " % c, end='', flush=True); time.sleep(0.05)
        print()
    sys.exit(0)
if cmd == 'update':
    print("** SKIPPING UPDATE:\nDevice is already running version 1.6.\nUse --force to update anyway."); sys.exit(0)
if cmd == 'convert':
    import os; os.execvp('gw', ['gw'] + a)
print("Command Failed: fake gw does not implement %s" % cmd); sys.exit(0)
