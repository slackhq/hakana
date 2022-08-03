<<\Hakana\SecurityAnalysis\Specialize>>
function data(dict<arraykey, mixed> $data, string $key) {
    return $data[$key];
}

<<\Hakana\SecurityAnalysis\Specialize>>
function get(string $key) {
    return data($_GET, $key);
}

echo get("x");