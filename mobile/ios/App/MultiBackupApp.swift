import SwiftUI
import WebKit

@main
struct MultiBackupApp: App {
    var body: some Scene {
        WindowGroup {
            ServerConsoleView()
        }
    }
}

struct ServerConsoleView: View {
    @AppStorage("serverOrigin") private var origin = ""
    @State private var entered = "https://"
    @State private var validationError = ""

    private func canonicalOrigin(_ input: String) -> String? {
        guard let url = URLComponents(string: input.trimmingCharacters(in: .whitespacesAndNewlines)),
              url.scheme?.lowercased() == "https",
              let host = url.host, !host.isEmpty,
              url.user == nil, url.password == nil,
              url.query == nil, url.fragment == nil,
              url.path.isEmpty || url.path == "/" else { return nil }
        var canonical = URLComponents()
        canonical.scheme = "https"
        canonical.host = host.lowercased()
        canonical.port = url.port
        return canonical.url?.absoluteString
    }

    var body: some View {
        NavigationStack {
            Group {
                if origin.isEmpty {
                    Form {
                        Section("Connessione al server") {
                            TextField("https://backup.example.org", text: $entered)
                                .keyboardType(.URL)
                                .textContentType(.URL)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                .accessibilityLabel("Indirizzo HTTPS server")
                            Button("Connetti") {
                                if let target = canonicalOrigin(entered) {
                                    validationError = ""
                                    origin = target
                                } else {
                                    validationError = "Indica solo l'origine HTTPS del server, senza credenziali, percorso o query."
                                }
                            }
                        }
                        Section {
                            Text("Le operazioni di backup avvengono sul server. Questa app visualizza la stessa console web, adattata a iPhone e iPad.")
                                .foregroundStyle(.secondary)
                        }
                        if !validationError.isEmpty {
                            Section {
                                Text(validationError).foregroundStyle(.red)
                            }
                        }
                    }
                } else {
                    RemoteServerWebView(origin: origin)
                        .id(origin)
                }
            }
            .navigationTitle("Multi Backup")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if !origin.isEmpty {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("Server") {
                            WKWebsiteDataStore.default().removeData(
                                ofTypes: WKWebsiteDataStore.allWebsiteDataTypes(),
                                modifiedSince: Date.distantPast
                            ) { }
                            origin = ""
                        }
                    }
                }
            }
        }
    }
}

struct RemoteServerWebView: UIViewRepresentable {
    let origin: String

    func makeCoordinator() -> Coordinator {
        Coordinator(origin: origin)
    }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .default()
        let webView = WKWebView(frame: .zero, configuration: config)
        webView.navigationDelegate = context.coordinator
        webView.allowsBackForwardNavigationGestures = true
        webView.scrollView.contentInsetAdjustmentBehavior = .automatic
        if let url = URL(string: origin + "/") {
            webView.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData))
        }
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) { }

    final class Coordinator: NSObject, WKNavigationDelegate {
        private let origin: URLComponents

        init(origin: String) {
            self.origin = URLComponents(string: origin)!
        }

        func webView(_ webView: WKWebView,
                     decidePolicyFor action: WKNavigationAction,
                     decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
            guard let target = action.request.url else {
                decisionHandler(.cancel)
                return
            }
            if target.scheme == "about" {
                decisionHandler(.allow)
                return
            }
            guard let components = URLComponents(url: target, resolvingAgainstBaseURL: false),
                  components.scheme?.lowercased() == "https" else {
                decisionHandler(.cancel)
                return
            }
            if components.host?.lowercased() == origin.host?.lowercased() &&
                components.port == origin.port {
                decisionHandler(.allow)
            } else {
                // Only deliberate external link clicks leave the authenticated WebView.
                if action.navigationType == .linkActivated {
                    UIApplication.shared.open(target)
                }
                decisionHandler(.cancel)
            }
        }
    }
}
