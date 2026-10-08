function replace_dynamic_key(string $text, string $needle, string $replacement): string {
    $replacements = dict[$needle => $replacement];
    return Str\replace_every($text, $replacements);
}

function replace_captured_key(string $text, string $replacement): string {
    $needle = 'fixture-id';
    $replace = (string $input) ==> Str\replace_every($input, dict[$needle => $replacement]);
    return $replace($text);
}
