function redirect(
    <<Hakana\SecurityAnalysis\Sink("RedirectUri")>> string $url
): noreturn {
    header("Location: " . $url);
    exit();
}

redirect(HH\global_get('_GET')['taint']);