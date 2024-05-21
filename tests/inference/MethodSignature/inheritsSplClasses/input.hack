namespace App;

use SplObserver;
use SplSubject;

final class Observer implements \SplObserver
{
    public function update(SplSubject $subject)
    {
    }
}

final class Subject implements \SplSubject
{
    public function attach(SplObserver $observer)
    {
    }

    public function detach(SplObserver $observer)
    {
    }

    public function notify()
    {
    }
}