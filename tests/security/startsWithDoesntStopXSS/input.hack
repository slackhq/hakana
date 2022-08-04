$url = $_GET['url'];

if (HH\Lib\Str\starts_with($url, '/foo')) {
    echo $url;
}
