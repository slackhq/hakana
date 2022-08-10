StringUtility::foo($_GET["c"]);

class StringUtility {
    <<\Hakana\SecurityAnalysis\SpecializeCall()>>
    public static function foo(string $str) : string
    {
        return $str;
    }

    <<\Hakana\SecurityAnalysis\SpecializeCall()>>
    public static function slugify(string $url) : string {
        return self::foo($url);
    }
}

echo StringUtility::slugify("hello");