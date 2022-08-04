$url = $_GET['url'];

if (HH\Lib\Str\starts_with($url, '/foo')) {
    $ch = curl_init($url);
}
