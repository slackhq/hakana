function slice_with_vars(HH\Lib\Locale\Locale $locale, string $text): string {
    $offset = 2;
    $length = 5;
    return HH\Lib\Str\slice_l($locale, $text, $offset, $length);
}

function slice_with_offset_var(HH\Lib\Locale\Locale $locale, string $text): string {
    $offset = 2;
    return HH\Lib\Str\slice_l($locale, $text, $offset);
}
