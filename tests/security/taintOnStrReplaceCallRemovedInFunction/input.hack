class U {
    <<\Hakana\SecurityAnalysis\Sanitize('HtmlTag')>>
    public static function shorten(string $s)[]: string {
        return str_replace("foo", "bar", $s);
    }
}

class V {}

class O1 {
    public string $s;

    public function __construct() {
        $this->s = (string) $_GET["FOO"];
    }
}

class V1 extends V {
    public function foo(O1 $o) : void {
        echo U::shorten($o->s);
    }
}