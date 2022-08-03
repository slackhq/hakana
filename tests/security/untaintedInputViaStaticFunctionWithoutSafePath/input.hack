class Utils {
    public static function shorten(string $str)[]: string {
        return $str;
    }
}

class A {
    public function foo() : void {
        echo(Utils::shorten((string) $_GET["user_id"]));
    }

    public function bar() : void {
        echo(Utils::shorten("hello"));
    }
}