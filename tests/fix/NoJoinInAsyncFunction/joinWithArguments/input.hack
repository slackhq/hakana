async function fetch_data(string $url): Awaitable<string> {
    return "data from " . $url;
}

async function fetch_multiple(vec<string> $urls): Awaitable<vec<string>> {
    $results = vec[];
    foreach ($urls as $url) {
        $results[] = HH\Asio\join(fetch_data($url));
    }
    return $results;
}

async function test_join_with_complex_argument(): Awaitable<void> {
    $result = HH\Asio\join(fetch_data("https://example.com/" . time()));
    echo $result;
}