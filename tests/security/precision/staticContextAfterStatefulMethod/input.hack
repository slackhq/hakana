final class UrlBuilder {
    private static string $state = '';

    public static function remember(string $input): void {
        self::$state = $input;
    }

    public static function root(string $path = ''): string {
        return 'https://example.com/' . $path;
    }
}

UrlBuilder::remember('safe');
UrlBuilder::root((string)HH\global_get('_GET')['path']);
echo UrlBuilder::root();
