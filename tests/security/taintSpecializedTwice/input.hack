function data(dict<arraykey, mixed> $data, string $key) {
    return $data[$key];
}

function get(string $key) {
    return data($_GET, $key);
}

echo get("x");