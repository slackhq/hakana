function highlight(string $needle, string $output)[] : string {
    $needle = preg_quote($needle, '#');
    $needles = str_replace(vec['"', ' '], vec['', '|'], $needle);
    $output = preg_replace("#({$needles})#im", "<mark>$1</mark>", $output) as nonnull;

    return $output;
}