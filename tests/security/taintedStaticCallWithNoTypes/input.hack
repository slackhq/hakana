class A {
    public static function rawinput() {
        return $_GET['rawinput'];
    }
}

echo A::rawinput();