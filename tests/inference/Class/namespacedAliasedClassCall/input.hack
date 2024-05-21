namespace Aye {
    final class Foo {}
}
namespace Bee {
    use Aye as A;

    new A\Foo();
}