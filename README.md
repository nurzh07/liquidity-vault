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
