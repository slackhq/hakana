final class U {
    <<\Hakana\SecurityAnalysis\Sanitize('HtmlTag')>>
    public static function shorten(string $s)[]: string {
        return str_replace("foo", "bar", $s);
    }
}

final class V {}

final class O1 {
    public string $s;

    public function __construct() {
        $this->s = (string) HH\global_get('_GET')["FOO"];
    }
}

final class V1 extends V {
    public function foo(O1 $o) : void {
        echo U::shorten($o->s);
    }
}