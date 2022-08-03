class Utils {
    public static function shorten(string $str)[]: string {
        return substr($str, 0, 100);
    }
}

class A {
    public function foo() : void {
        echo(Utils::shorten((string) $_GET["user_id"]));
    }
}