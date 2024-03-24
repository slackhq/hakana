function generate_diff(string $wanted_re): mixed {
    $m = null;
    preg_match_with_matches('/^\((.*)\)\{(\d+)\}$/s', $wanted_re, inout $m);
    return $m;
}