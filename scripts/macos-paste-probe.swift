// What WKWebView, Escouade's web view under macOS, gives a page's paste: Cmd+V goes through the Edit
// menu's Paste, which sends `paste:` to the web view, as this probe does. The page keeps what the
// composer (Composer.svelte, onPaste) and the Claude Code view (terminals.ts, pastedContent) read:
// the files of the paste, their type, and its text.
//
// swiftc -o paste-probe scripts/macos-paste-probe.swift && ./paste-probe image|text
// Prints the paste as JSON; exits 0 when it is what `image` (an image file the composer takes, no
// text) or `text` (text, no file) expects, 1 otherwise, 2 without a paste in 20 s.

import AppKit
import WebKit

let expected = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "image"

let page = """
<!doctype html>
<textarea autofocus></textarea>
<script>
const IMAGES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];
document.querySelector('textarea').addEventListener('paste', async (e) => {
  const data = e.clipboardData;
  const files = [...(data?.files ?? [])];
  const text = data?.getData('text/plain') ?? '';
  const head = files[0] ? [...new Uint8Array(await files[0].slice(0, 8).arrayBuffer())] : [];
  const png = head.slice(0, 4).join() === '137,80,78,71';
  const report = {
    types: [...(data?.types ?? [])],
    files: files.map((f) => ({ name: f.name, type: f.type, size: f.size })),
    text,
    png,
  };
  const ok = '\(expected)' === 'text'
    ? text !== '' && files.length === 0
    : text === '' && files.length > 0 && IMAGES.includes(files[0].type) && files[0].size > 0 && png;
  window.webkit.messageHandlers.probe.postMessage(JSON.stringify({ ok, ...report }));
});
</script>
"""

final class Probe: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
  var window: NSWindow!
  var web: WKWebView!

  override init() {
    super.init()
    // On the configuration the web view is made with (its `configuration` gives a copy).
    let config = WKWebViewConfiguration()
    config.userContentController.add(self, name: "probe")
    let frame = NSRect(x: 0, y: 0, width: 480, height: 320)
    web = WKWebView(frame: frame, configuration: config)
    window = NSWindow(contentRect: frame, styleMask: [.titled], backing: .buffered, defer: false)
    web.navigationDelegate = self
    window.contentView = web
    window.makeKeyAndOrderFront(nil)
    web.loadHTMLString(page, baseURL: nil)
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    window.makeFirstResponder(web)
    web.evaluateJavaScript("document.querySelector('textarea').focus()") { _, _ in
      // The Edit menu's Paste: `paste:` down the responder chain, from the web view.
      if !NSApp.sendAction(#selector(NSText.paste(_:)), to: nil, from: nil) {
        NSApp.sendAction(#selector(NSText.paste(_:)), to: self.web, from: nil)
      }
    }
  }

  func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
    let body = message.body as? String ?? "{}"
    print(body)
    let ok = (try? JSONSerialization.jsonObject(with: Data(body.utf8)) as? [String: Any])?["ok"] as? Bool ?? false
    exit(ok ? 0 : 1)
  }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let probe = Probe()
app.activate(ignoringOtherApps: true)
DispatchQueue.main.asyncAfter(deadline: .now() + 20) {
  print("{\"ok\":false,\"error\":\"no paste event in 20 s\"}")
  exit(2)
}
app.run()
