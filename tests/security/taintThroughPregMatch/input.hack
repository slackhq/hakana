function foo(): void {
    $text = HH\global_get('_GET')['bad'];
    $matches = dict[];
    if (
        \preg_match_all_with_matches(
            '/[^"]+/',
            $text,
            inout $matches,
        )
    ) {
        foreach ($matches[1] as $match) {
            echo $match;
        }
    }
}