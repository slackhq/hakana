<<\Hakana\SecurityAnalysis\SpecializeCall()>>
function data(dict<arraykey, mixed> $data, string $key) {
    return $data[$key];
}

<<\Hakana\SecurityAnalysis\SpecializeCall()>>
function get(string $key) {
    return data($_GET, $key);
}

echo get("x");