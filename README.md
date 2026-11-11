# Liquidity Vault

Учебный DeFi-проект на Solana (Anchor 1.x): vault принимает токены пользователей, хранит их в общем «котле» и выдаёт взамен share-токены (квитанцию с долей). Позже администратор будет размещать средства в CLMM-пуле Raydium.

**Статус: неделя 8 (РК1), каркас vault.** Деньги пока не двигаются: создана только структура аккаунтов.

Сеть: **Solana Devnet**.

## Адреса в Devnet

| Что | Адрес |
|-----|-------|
| Программа | [C9pH3cWmgQihD2xPQgWxsb7QyWkNpXQNPxsCBkVJrQM1](https://explorer.solana.com/address/C9pH3cWmgQihD2xPQgWxsb7QyWkNpXQNPxsCBkVJrQM1?cluster=devnet) |
| Депозитный токен (deposit mint) | [9pqQ6nSjAmt2wWzmXm8wg9h7Xek1kig593NkJ3RfgrVs](https://explorer.solana.com/address/9pqQ6nSjAmt2wWzmXm8wg9h7Xek1kig593NkJ3RfgrVs?cluster=devnet) |
| Vault (настройки, PDA) | [FpqwK4okMScfCTKEhtWdiyLLTYyv3JuW4bFWerKUQTVo](https://explorer.solana.com/address/FpqwK4okMScfCTKEhtWdiyLLTYyv3JuW4bFWerKUQTVo?cluster=devnet) |
| Share-токен (PDA) | [4kcfGXCJJvQLhwRr87cG5E8nBqSGdtPZ7HVTCXxpYPVW](https://explorer.solana.com/address/4kcfGXCJJvQLhwRr87cG5E8nBqSGdtPZ7HVTCXxpYPVW?cluster=devnet) |
| Хранилище токенов (PDA) | [7XiyLv9xNmvDLg1FFDmW7NigwvK4M1dtf5pSJLvbtSKN](https://explorer.solana.com/address/7XiyLv9xNmvDLg1FFDmW7NigwvK4M1dtf5pSJLvbtSKN?cluster=devnet) |
| Карточка пользователя (PDA) | [FrT2D61wJcQ3W515cUFPPVCAXPEtvNrUPgnAqYU4AiF6](https://explorer.solana.com/address/FrT2D61wJcQ3W515cUFPPVCAXPEtvNrUPgnAqYU4AiF6?cluster=devnet) |

## Схема аккаунтов



## Вопросы для самопроверки

**Почему share-токен выпускает программа, а не администратор?**
Mint authority share-токена это PDA vault, а подписать за PDA может только программа. Если бы выпускал администратор, он мог бы печатать share-токены без внесения токенов и забрать чужие деньги. Программа выпускает share-токены только в депозите, по формуле.

**Что такое PDA и почему у неё нет приватного ключа?**
PDA (Program Derived Address) это адрес, вычисленный из seeds и Program Id и намеренно выбранный так, чтобы он не лежал на кривой ed25519. Поэтому для него не существует приватного ключа. Подтвердить подпись за PDA может только программа, передав те же seeds.

**Как найти карточку конкретного пользователя?**
Пересчитать адрес: `findProgramAddress(["position", vault, user], programId)`. Хранить список всех пользователей не нужно.

## Безопасность

В репозитории нет ключей кошельков и seed-фраз. Работаем только в devnet.

## Неделя 9: депозит и share-токены

Инструкция `deposit(amount)`:
1. `amount > 0`, иначе ошибка `ZeroAmount`;
2. считаем share-токены: первому вкладчику столько же, сколько внёс, дальше `amount * share_supply / total_assets` (в `u128`, без переполнения);
3. CPI в Token Program: `TransferChecked` из токенов пользователя в хранилище vault (**подписывает пользователь**);
4. CPI в Token Program: `MintTo` share-токенов пользователю (**подписывает программа** от имени PDA vault, `with_signer`);
5. обновляем `vault.total_deposited` и `position.deposited`.

Транзакции в Devnet:
- [Депозит 1: внесено 1000, получено 1000 share](https://explorer.solana.com/tx/qFqBQTbqQVGWTh1qryTATKFctE7qpxf4Qw2rE9Y8dfKjgJmQfYHEmkk7Tn9ukCjvhbweJmuDh4fxtAeAa7F8VMH?cluster=devnet)
- [Имитация дохода: +100 токенов напрямую в хранилище](https://explorer.solana.com/tx/5DFsdnEDYZQ3tAtJ8o3LQKu6hkaEYA8k1LJP1ru33FHTT7irAL7KdyX2ST1ZJgDJVZ7aGUynRv9mGZ3qB1rYJH3S?cluster=devnet)
- [Депозит 2: внесено 110, получено 100 share](https://explorer.solana.com/tx/5rWjv7zaMj5EDgDwyY6Sththp79Kipaqmh8Z1Tc8wsbPmGyL69oxgZLvVS6AACGxrV1j7egsUZQ8xTZzaCpoULCe?cluster=devnet)

Доход vault в тесте имитируется прямым переводом токенов в хранилище (настоящие комиссии появятся на неделе 13). Результат: 1210 токенов в хранилище, 1100 share-токенов, цена share = 1,1.

### Вопросы для самопроверки
**Почему поздний вкладчик может получить share-токенов меньше, чем внёс токенов?**
После дохода каждый share-токен стоит дороже (1210 / 1100 = 1,1). Доход принадлежит тем, кто уже внёс токены, поэтому новый вкладчик получает долю по текущей цене и не забирает чужой доход.

**Почему перевод подписывает пользователь, а выпуск share-токенов программа?**
Токены пользователя можно двигать только с подписью их владельца. Mint authority share-токена это PDA vault, у которого нет приватного ключа, и подписать за него может только программа (signer seeds). Поэтому share-токены нельзя выпустить без депозита.

**Какие программы участвовали в транзакции депозита?**
Наша программа `liquidity_vault` и Token Program: во внутренних инструкциях (CPI) `TransferChecked` и `MintTo`. При первом депозите добавляются Associated Token Program и System Program для создания токен-аккаунта пользователя.

## Неделя 10: вывод из vault (CPI)

Инструкция `withdraw(shares)`:
1. `shares > 0` и не больше, чем share-токенов у пользователя;
2. считаем долю: `amount_out = shares * токены_в_vault / все_share` (в `u128`);
3. **CPI в Token Program: `Burn`**, сжигаем share-токены пользователя. Подписывает **пользователь**;
4. **CPI в Token Program: `TransferChecked`**, переводим токены из хранилища vault пользователю. Подписывает **программа** от имени PDA vault (`with_signer` с seeds `["vault", deposit_mint, bump]`);
5. уменьшаем `position.deposited` и `vault.total_deposited` пропорционально выведенным share-токенам.

Транзакция вывода в Devnet:
[Withdraw: 550 share сожжено, выдано 605 токенов](https://explorer.solana.com/tx/4zgvH4VD9SdZztmVW7RBoEDQ1JWaX6xgcRRqZMmYNkP9JJwQ528CsHrjm3cfNwtvJxRvZMhGN9m72epD5QY4C4RZ?cluster=devnet)

Результат: в vault было 1210 токенов и 1100 share, после вывода 605 токенов и 550 share. Цена share осталась 1,1: выход одного пользователя не меняет цену для остальных.

### Какие CPI-вызовы видны в транзакции вывода

В Explorer (Programs & Logs / Inspect) видна одна внешняя инструкция нашей программы `liquidity_vault` и две внутренние (inner instructions) в Token Program:
- `Burn`: сжигает share-токены, authority = кошелёк пользователя (его подпись «передаётся» дальше);
- `TransferChecked`: переводит токены из `vault_token` пользователю, authority = PDA vault (за него подписывает программа).

### Вопросы для самопроверки

**Что такое CPI и зачем он нужен?**
CPI (Cross-Program Invocation) это вызов одной программы из другой. Наша программа сама не умеет двигать токены, это делает Token Program, поэтому vault «просит» её сжечь share-токены и перевести токены. Так программа пользуется готовыми программами и не пишет всё с нуля (дальше таким же образом будем вызывать Raydium).

**Когда подписывает пользователь, а когда программа?**
Пользователь подписывает, когда двигаются его собственные токены или share-токены (`Burn`, а в депозите `TransferChecked` из его токен-аккаунта). Программа подписывает от имени PDA vault, когда двигаются токены vault (`TransferChecked` из хранилища при выводе) или выпускаются share-токены (`MintTo` при депозите).

**Почему администратор не может просто забрать токены из vault своей подписью?**
Хранилищем владеет PDA vault, а у PDA нет приватного ключа, поэтому никакая подпись кошелька (в том числе администратора) не может его распечатать. Подписать за PDA может только наша программа, и она делает это только внутри `withdraw` по формуле доли, в обмен на сжигание share-токенов пользователя.
