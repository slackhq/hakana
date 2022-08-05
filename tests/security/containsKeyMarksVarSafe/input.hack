function foo(): void {
    $url = $_GET['url'];

    $acceptable_urls = dict["https://google.com" => 1, "https://vimeo.com" => 2];

    if (HH\Lib\C\contains_key($acceptable_urls, $url)) {
        $ch = curl_init($url);
    }
}
