function takes_string(string $haystack): void {
    preg_match_with_matches("/^foo$/", $haystack, inout $matches);
}
