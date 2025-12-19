function foo(string $text): string {
    $matches = \HH\Lib\Regex\first_match(
        $text,
        re"{
        mailto:
        (
            (?:
                [-!#$%&\'*+/=?^_`.{|}~\w\x80-\xFF]+
            |
                \".*?\"
            )
            \@
            (?:
                [-a-z0-9\x80-\xFF]+(\.[-a-z0-9\x80-\xFF]+)*\.[a-z]+
            |
                \[[\d.a-fA-F:]+\] # IPv4 & IPv6
            )
        )
        }xi",
    );
    if ($matches is nonnull && $text != "{$matches[0]}|{$matches[1]}") {
        return '<'.$text.'|'.$matches[1].'>';
    }
    return '<'.$text.'>';
}