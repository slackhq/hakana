function foo(): void {
    $text = HH\global_get('_GET')['bad'];
    $matches = dict[];
    if (
        \preg_match_all_with_matches(
            /* HAKANA_SECURITY_IGNORE[HtmlTag] */
            '!<@([WU]+[0-9A-Z]+)!',
            $text,
            inout $matches,
        )
    ) {
        foreach ($matches[1] as $match) {
            echo $match;
        }
    }
}