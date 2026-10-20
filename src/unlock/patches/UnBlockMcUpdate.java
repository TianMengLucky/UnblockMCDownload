package org.jackhuang.hmcl.upgrade;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.URL;
import java.net.URLConnection;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** Runs the local patch helper after the official download has been verified. */
public final class UnBlockMcUpdate {
    private UnBlockMcUpdate() { }

    public static void prepare(Path downloaded, Path current) throws IOException {
        Path tool = Paths.get("__UNBLOCKMC_HELPER__");
        ensureTool(tool, "__UNBLOCKMC_RELEASE__");
        Process process = new ProcessBuilder(tool.toString(), "--patch-update",
                downloaded.toAbsolutePath().toString(), current.toAbsolutePath().toString())
                .inheritIO().start();
        try {
            if (process.waitFor() != 0)
                throw new IOException("UnBlockMC could not patch the downloaded update; the current launcher was kept.");
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new IOException("UnBlockMC update patch interrupted", error);
        }
    }

    private static URLConnection connect(String url) throws IOException {
        URLConnection connection = new URL(url).openConnection();
        connection.setConnectTimeout(15000);
        connection.setReadTimeout(30000);
        connection.setRequestProperty("User-Agent", "UnblockMCDownload-Updater");
        return connection;
    }

    private static void ensureTool(Path tool, String release) throws IOException {
        if (Files.isRegularFile(tool)) return;
        byte[] checksumBytes = new byte[4096];
        int count = 0;
        try (InputStream input = connect(release + "/MC.exe.sha256").getInputStream()) {
            int n;
            while (count < checksumBytes.length && (n = input.read(checksumBytes, count, checksumBytes.length - count)) != -1)
                count += n;
            if (count == checksumBytes.length) throw new IOException("Release checksum is too large");
        }
        Matcher checksum = Pattern.compile("\\A([a-fA-F0-9]{64})\\s+\\*?解锁MC下载限制\\.exe\\s*\\z")
                .matcher(new String(checksumBytes, 0, count, StandardCharsets.UTF_8));
        if (!checksum.matches()) throw new IOException("Invalid release checksum");
        Files.createDirectories(tool.getParent());
        Path temporary = Files.createTempFile(tool.getParent(), ".unblockmc-", ".tmp");
        try {
            MessageDigest hash = MessageDigest.getInstance("SHA-256");
            long length = 0;
            try (InputStream input = connect(release + "/MC.exe").getInputStream();
                 OutputStream output = Files.newOutputStream(temporary)) {
                byte[] buffer = new byte[65536];
                int n;
                while ((n = input.read(buffer)) != -1) {
                    length += n;
                    if (length > 268435456) throw new IOException("Release file is too large");
                    hash.update(buffer, 0, n);
                    output.write(buffer, 0, n);
                }
            }
            StringBuilder actual = new StringBuilder();
            for (byte value : hash.digest()) actual.append(String.format("%02x", value & 255));
            if (length < 2 || !actual.toString().equalsIgnoreCase(checksum.group(1)))
                throw new IOException("Downloaded tool failed SHA-256 verification");
            Files.move(temporary, tool);
        } catch (NoSuchAlgorithmException error) {
            throw new IOException("SHA-256 is unavailable", error);
        } finally {
            Files.deleteIfExists(temporary);
        }
    }
}
