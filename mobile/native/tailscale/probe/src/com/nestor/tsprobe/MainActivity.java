package com.nestor.tsprobe;

import android.app.Activity;
import android.os.Bundle;
import android.text.InputType;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.InputStream;

/** Sonde : un nœud Tailscale embarque (libtailscale) peut-il joindre le tailnet ? */
public class MainActivity extends Activity {
    private TextView output;
    private ScrollView scroll;
    private EditText authKey;
    private EditText target;
    private Button run;
    private final StringBuilder report = new StringBuilder();

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setPadding(32, 48, 32, 32);

        authKey = new EditText(this);
        authKey.setHint("Cle d'authentification Tailscale (tskey-auth-...)");
        authKey.setSingleLine(true);
        authKey.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);
        authKey.setText(defaultKey());
        root.addView(authKey);

        target = new EditText(this);
        target.setHint("Cible TCP (hote:port)");
        target.setSingleLine(true);
        target.setText("kanto.felis-ionian.ts.net:443");
        root.addView(target);

        run = new Button(this);
        run.setText("Tester le tunnel");
        run.setOnClickListener(v -> start());
        root.addView(run);

        output = new TextView(this);
        output.setTextIsSelectable(true);
        output.setTypeface(android.graphics.Typeface.MONOSPACE);
        output.setTextSize(12f);
        scroll = new ScrollView(this);
        scroll.addView(output);
        root.addView(scroll, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
        setContentView(root);
        log("Pret. Colle une cle ephemere (reutilisable) puis lance le test.");
    }

    /** Cle optionnelle fournie au build (assets/authkey.txt) : jamais commitee. */
    private String defaultKey() {
        try (InputStream in = getAssets().open("authkey.txt")) {
            return new String(in.readAllBytes()).trim();
        } catch (Exception e) {
            return "";
        }
    }

    private void log(String line) {
        runOnUiThread(() -> {
            report.append(line).append('\n');
            output.setText(report);
            scroll.post(() -> scroll.fullScroll(ScrollView.FOCUS_DOWN));
        });
    }

    private void start() {
        final String key = authKey.getText().toString().trim();
        final String addr = target.getText().toString().trim();
        if (key.isEmpty()) {
            log("Cle d'authentification manquante.");
            return;
        }
        run.setEnabled(false);
        new Thread(() -> {
            long t0 = System.currentTimeMillis();
            String dir = getFilesDir().getAbsolutePath() + "/tsnet";
            new java.io.File(dir).mkdirs();

            // Garde-fou : tailscale_up peut bloquer sans fin si la cle est refusee.
            Thread watchdog = new Thread(() -> {
                try { Thread.sleep(90_000); } catch (InterruptedException e) { return; }
                log("Delai de 90 s depasse : abandon de tailscale_up.");
                Native.close();
            });
            watchdog.start();

            log("--- tailscale_up (hostname nestor-probe) ---");
            String up = Native.up(dir, "nestor-probe", key);
            watchdog.interrupt();
            log(up + "  [" + (System.currentTimeMillis() - t0) + " ms]");

            if (up.startsWith("OK")) {
                log("--- tailscale_dial " + addr + " ---");
                log(Native.dial(addr, "GET / HTTP/1.0\r\n\r\n"));
                log("VERDICT : le tunnel embarque FONCTIONNE sur ce telephone.");
            } else {
                log("VERDICT : le tunnel embarque ne demarre pas (voir journaux).");
            }
            log("--- journaux libtailscale (fin) ---");
            String logs = Native.logs();
            int from = Math.max(0, logs.length() - 3000);
            log(logs.substring(from));
            runOnUiThread(() -> run.setEnabled(true));
        }).start();
    }
}
