// 划词长条按钮：五个功能等宽排开。点击由后端的鼠标钩子处理（窗口本身收不到点击、也不会抢焦点），
// 后端按点击位置算出第几段，所以这里各段必须等宽、顺序和后端 TABS 一致。
const ITEMS = ["翻译", "总结", "改写", "回复", "朗读"];

export default function Bubble() {
  return (
    <div className="bubble">
      {ITEMS.map((name) => (
        <span key={name}>{name}</span>
      ))}
    </div>
  );
}
