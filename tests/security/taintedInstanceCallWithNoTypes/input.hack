final class A {
    public function rawinput() {
        return $_GET['rawinput'];
    }
}

echo (new A())->rawinput();