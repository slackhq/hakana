function make_bad_utf8(string $something): string {
    $val = 'foo';
    $val .= '%' . "\xe2\xe3\xcf\xd3";
    return $something . $val;
}
