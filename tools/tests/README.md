# Browser layout regression

Requires Python Playwright and Chromium. Start Vite from the repository root:

```
npm ci
npm run dev -- --host 127.0.0.1 --port 5175
```

In another terminal:

```
python tools/tests/android_layout.py --output /tmp/kouros-layout
python tools/tests/android_layout.py --empty-only --output /tmp/kouros-empty-layout
```

The script uses fictional browser demo data. It opens the main screens, their
subtabs, workout details, import preview and help chapters at 360×800, 412×915,
915×412 and 1440×1000. It waits for asynchronous rendering to settle, then requires
three stable document-height samples and limits the mobile tab bar to its
intrinsic height. This catches the unbounded ResizeObserver/grid stretch feedback
that pushed health, training and nutrition downwards. The optional output stores
geometry and screenshots, not native health exports.

The empty-only pass checks the health screen without imported records. These are
browser layout tests, not tests of Android permissions, data providers, native
biometrics or device keyboard behaviour. Use --chromium for another Chromium path
and --url for another Vite port.

## Update UI

With Vite running, `python tools/tests/android_updates.py` uses mocked Android IPC
to check scheduled discovery, transient errors, install retry, system handoff,
manual checks and the automatic-check preference. Native URL/checksum/signer
policy is covered separately by JVM tests; these browser tests do not install APKs.
