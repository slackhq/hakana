namespace App;

use SplObserver;
use SplSubject;

final class Observer implements \SplObserver
{
    <<__Override>>
    public function update(SplSubject $subject)
    {
    }
}

final class Subject implements \SplSubject
{
    <<__Override>>
    public function attach(SplObserver $observer)
    {
    }

    <<__Override>>
    public function detach(SplObserver $observer)
    {
    }

    <<__Override>>
    public function notify()
    {
    }
}