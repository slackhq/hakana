<<\Hakana\SecurityAnalysis\Sanitize('HtmlTag')>>
function escapeHtml(string $arg): string {
    return htmlspecialchars($arg);
}

$tainted = $_GET['foo'];

try {
    $tainted = escapeHtml($tainted);
} catch (Throwable $_) {
}

echo $tainted;
                