namespace Aye {
    class Foo {}
}
namespace Bee {
    use Aye as A;

    new A\Foo();
}