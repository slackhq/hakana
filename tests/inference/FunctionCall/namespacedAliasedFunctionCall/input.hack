namespace Aye {
    function foo(): void { }
}
namespace Bee {
    use Aye as A;

    A\foo();
}