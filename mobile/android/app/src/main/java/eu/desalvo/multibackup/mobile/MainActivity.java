package eu.desalvo.multibackup.mobile;

import android.app.Activity;
import android.content.Intent;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.CookieManager;
import android.webkit.SslErrorHandler;
import android.net.http.SslError;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.net.URI;
import java.net.URISyntaxException;
import java.util.Locale;

/** Native HTTPS-only remote server console; no backup data or age keys on device. */
public final class MainActivity extends Activity {
    private static final String PREFS = "multi_backup_mobile";
    private static final String SERVER = "server_origin";
    private WebView webView;
    private String origin;

    private static String normalizeOrigin(String entered) throws URISyntaxException {
        URI input = new URI(entered.trim());
        if (!"https".equalsIgnoreCase(input.getScheme()) ||
                input.getHost() == null || input.getHost().isEmpty() ||
                input.getRawUserInfo() != null || input.getRawQuery() != null ||
                input.getRawFragment() != null ||
                !(input.getRawPath() == null || input.getRawPath().isEmpty() || "/".equals(input.getRawPath())) ||
                input.getPort() > 65535) {
            throw new URISyntaxException(entered, "Enter a server HTTPS origin only");
        }
        return new URI("https", null, input.getHost().toLowerCase(Locale.ROOT),
                input.getPort(), null, null, null).toString();
    }

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        origin = getSharedPreferences(PREFS, MODE_PRIVATE).getString(SERVER, "");
        if (origin.isEmpty()) { showSetup(); }
        else { showConsole(); }
    }

    private LinearLayout container() {
        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        layout.setBackgroundColor(Color.rgb(8, 23, 49));
        layout.setPadding(12, 12, 12, 12);
        setContentView(layout);
        return layout;
    }

    private void showSetup() {
        LinearLayout layout = container();
        TextView heading = new TextView(this);
        heading.setText("Multi Backup · Server remoto");
        heading.setTextSize(24);
        heading.setTextColor(Color.WHITE);
        layout.addView(heading);
        TextView help = new TextView(this);
        help.setText("Inserisci l'indirizzo HTTPS del server. Nessuna chiave di backup viene memorizzata sul dispositivo.");
        help.setTextColor(Color.WHITE);
        help.setPadding(0, 12, 0, 12);
        layout.addView(help);
        EditText address = new EditText(this);
        address.setSingleLine(true);
        address.setText("https://");
        address.setTextColor(Color.WHITE);
        address.setHintTextColor(Color.LTGRAY);
        address.setInputType(android.text.InputType.TYPE_TEXT_VARIATION_URI);
        layout.addView(address);
        TextView error = new TextView(this);
        error.setTextColor(Color.rgb(255, 181, 181));
        layout.addView(error);
        Button connect = new Button(this);
        connect.setText("Connetti");
        layout.addView(connect);
        connect.setOnClickListener(view -> {
            try {
                String target = normalizeOrigin(address.getText().toString());
                origin = target;
                getSharedPreferences(PREFS, MODE_PRIVATE).edit().putString(SERVER, target).apply();
                showConsole();
            } catch (URISyntaxException ex) {
                error.setText("Usa esclusivamente https://nome-server[:porta], senza percorsi o credenziali.");
            }
        });
    }

    private void showConsole() {
        final String serverOrigin = origin;
        LinearLayout layout = container();
        LinearLayout bar = new LinearLayout(this);
        bar.setOrientation(LinearLayout.HORIZONTAL);
        TextView title = new TextView(this);
        title.setText("Multi Backup · Remoto");
        title.setTextColor(Color.WHITE);
        title.setTextSize(17);
        bar.addView(title, new LinearLayout.LayoutParams(0,
                ViewGroup.LayoutParams.WRAP_CONTENT, 1));
        Button change = new Button(this);
        change.setText("Server");
        change.setOnClickListener(view -> {
            if (webView != null) {
                webView.stopLoading();
                webView.destroy();
                webView = null;
            }
            CookieManager.getInstance().removeAllCookies(null);
            origin = "";
            getSharedPreferences(PREFS, MODE_PRIVATE).edit().remove(SERVER).apply();
            showSetup();
        });
        bar.addView(change);
        layout.addView(bar);

        webView = new WebView(this);
        WebSettings settings = webView.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setAllowFileAccess(false);
        settings.setAllowContentAccess(false);
        settings.setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
        settings.setSafeBrowsingEnabled(true);
        webView.setWebChromeClient(new WebChromeClient());
        WebView.setWebContentsDebuggingEnabled(false);
        CookieManager.getInstance().setAcceptCookie(true);
        webView.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                if (!request.isForMainFrame()) return false;
                Uri url = request.getUrl();
                if ("https".equalsIgnoreCase(url.getScheme())) {
                    try {
                        URI target = new URI(url.toString());
                        String targetOrigin = new URI("https", null, target.getHost(),
                                target.getPort(), null, null, null).toString();
                        if (serverOrigin.equals(normalizeOrigin(targetOrigin))) return false;
                    } catch (URISyntaxException ignored) { /* deny invalid URL */ }
                    // Deliberate external links leave the authenticated WebView.
                    if (request.hasGesture()) {
                        Intent intent = new Intent(Intent.ACTION_VIEW, url);
                        if (intent.resolveActivity(getPackageManager()) != null) startActivity(intent);
                    }
                }
                return true;
            }
            @Override public void onReceivedSslError(WebView view, SslErrorHandler handler, SslError error) {
                handler.cancel(); // No self-signed TLS bypass on mobile.
            }
        });
        layout.addView(webView, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1));
        webView.loadUrl(serverOrigin + "/");
    }

    @Override public void onBackPressed() {
        if (webView != null && webView.canGoBack()) webView.goBack();
        else super.onBackPressed();
    }

    @Override protected void onDestroy() {
        if (webView != null) { webView.destroy(); webView = null; }
        super.onDestroy();
    }
}
