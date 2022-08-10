function redirect(
    <<Hakana\SecurityAnalysis\Sink("RedirectUri")>> string $url
): noreturn {
    header("Location: " . $url);
    exit();
}

redirect($_GET['taint']);