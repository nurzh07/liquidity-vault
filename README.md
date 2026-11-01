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
