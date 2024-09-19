<<\Hakana\SecurityAnalysis\Sanitize('HtmlTag')>>
function escapeHtml(string $arg): string {
    return htmlspecialchars($arg);
}

$tainted = HH\global_get('_GET')['foo'];

try {
    $tainted = escapeHtml($tainted);
} catch (Throwable $_) {
}

echo $tainted;
                