function foo(dict<int, string> $tokens): void {
    $comment = null;
    $inline = false;
    $count = rand(0, 100);
    $quote = null;

    foreach ($tokens as $token) {
        if (rand(0, 1)) {
            continue;
        }

        if ($quote === null) {}

        if (rand(0, 1)) {
            $quote = $token;
        }

        if ($quote !== null) {
            continue;
        }
    }
}