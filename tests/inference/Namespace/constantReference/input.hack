namespace Aye\Bee {
    const HELLO = "hello";
}
namespace Aye\Bee {
    function foo(): void {
        echo \Aye\Bee\HELLO;
    }

    class Bar {
        public function foo(): void {
            echo \Aye\Bee\HELLO;
        }
    }
}