-- Flag chat badges. Country/region flags use Unicode regional indicator pairs
-- so the seed stays compact; other standard flag emoji are listed explicitly.
-- Same marketplace item shape as the existing badge shop:
-- item_kind=badge, slot=chat_badge, basic=1000 chips.
WITH extra_flag_seed(sku, emoji, sort_order) AS (
    VALUES
        ('chequered', '🏁', 1380),
        ('triangular', '🚩', 1381),
        ('crossed', '🎌', 1382),
        ('black', '🏴', 1383),
        ('white', '🏳️', 1384),
        ('rainbow', '🏳️‍🌈', 1385),
        ('transgender', '🏳️‍⚧️', 1386),
        ('pirate', '🏴‍☠️', 1387)
),
subdivision_flag_seed(sku, tag, sort_order) AS (
    VALUES
        ('england', 'gbeng', 1388),
        ('scotland', 'gbsct', 1389),
        ('wales', 'gbwls', 1390)
),
regional_flag_seed(code, sort_order) AS (
    VALUES
        ('AC', 1400), ('AD', 1401), ('AE', 1402), ('AF', 1403),
        ('AG', 1404), ('AI', 1405), ('AL', 1406), ('AM', 1407),
        ('AO', 1408), ('AQ', 1409), ('AR', 1410), ('AS', 1411),
        ('AT', 1412), ('AU', 1413), ('AW', 1414), ('AX', 1415),
        ('AZ', 1416), ('BA', 1417), ('BB', 1418), ('BD', 1419),
        ('BE', 1420), ('BF', 1421), ('BG', 1422), ('BH', 1423),
        ('BI', 1424), ('BJ', 1425), ('BL', 1426), ('BM', 1427),
        ('BN', 1428), ('BO', 1429), ('BQ', 1430), ('BR', 1431),
        ('BS', 1432), ('BT', 1433), ('BV', 1434), ('BW', 1435),
        ('BY', 1436), ('BZ', 1437), ('CA', 1438), ('CC', 1439),
        ('CD', 1440), ('CF', 1441), ('CG', 1442), ('CH', 1443),
        ('CI', 1444), ('CK', 1445), ('CL', 1446), ('CM', 1447),
        ('CN', 1448), ('CO', 1449), ('CP', 1450), ('CR', 1451),
        ('CU', 1452), ('CV', 1453), ('CW', 1454), ('CX', 1455),
        ('CY', 1456), ('CZ', 1457), ('DE', 1458), ('DG', 1459),
        ('DJ', 1460), ('DK', 1461), ('DM', 1462), ('DO', 1463),
        ('DZ', 1464), ('EA', 1465), ('EC', 1466), ('EE', 1467),
        ('EG', 1468), ('EH', 1469), ('ER', 1470), ('ES', 1471),
        ('ET', 1472), ('EU', 1473), ('FI', 1474), ('FJ', 1475),
        ('FK', 1476), ('FM', 1477), ('FO', 1478), ('FR', 1479),
        ('GA', 1480), ('GB', 1481), ('GD', 1482), ('GE', 1483),
        ('GF', 1484), ('GG', 1485), ('GH', 1486), ('GI', 1487),
        ('GL', 1488), ('GM', 1489), ('GN', 1490), ('GP', 1491),
        ('GQ', 1492), ('GR', 1493), ('GS', 1494), ('GT', 1495),
        ('GU', 1496), ('GW', 1497), ('GY', 1498), ('HK', 1499),
        ('HM', 1500), ('HN', 1501), ('HR', 1502), ('HT', 1503),
        ('HU', 1504), ('IC', 1505), ('ID', 1506), ('IE', 1507),
        ('IL', 1508), ('IM', 1509), ('IN', 1510), ('IO', 1511),
        ('IQ', 1512), ('IR', 1513), ('IS', 1514), ('IT', 1515),
        ('JE', 1516), ('JM', 1517), ('JO', 1518), ('JP', 1519),
        ('KE', 1520), ('KG', 1521), ('KH', 1522), ('KI', 1523),
        ('KM', 1524), ('KN', 1525), ('KP', 1526), ('KR', 1527),
        ('KW', 1528), ('KY', 1529), ('KZ', 1530), ('LA', 1531),
        ('LB', 1532), ('LC', 1533), ('LI', 1534), ('LK', 1535),
        ('LR', 1536), ('LS', 1537), ('LT', 1538), ('LU', 1539),
        ('LV', 1540), ('LY', 1541), ('MA', 1542), ('MC', 1543),
        ('MD', 1544), ('ME', 1545), ('MF', 1546), ('MG', 1547),
        ('MH', 1548), ('MK', 1549), ('ML', 1550), ('MM', 1551),
        ('MN', 1552), ('MO', 1553), ('MP', 1554), ('MQ', 1555),
        ('MR', 1556), ('MS', 1557), ('MT', 1558), ('MU', 1559),
        ('MV', 1560), ('MW', 1561), ('MX', 1562), ('MY', 1563),
        ('MZ', 1564), ('NA', 1565), ('NC', 1566), ('NE', 1567),
        ('NF', 1568), ('NG', 1569), ('NI', 1570), ('NL', 1571),
        ('NO', 1572), ('NP', 1573), ('NR', 1574), ('NU', 1575),
        ('NZ', 1576), ('OM', 1577), ('PA', 1578), ('PE', 1579),
        ('PF', 1580), ('PG', 1581), ('PH', 1582), ('PK', 1583),
        ('PL', 1584), ('PM', 1585), ('PN', 1586), ('PR', 1587),
        ('PS', 1588), ('PT', 1589), ('PW', 1590), ('PY', 1591),
        ('QA', 1592), ('RE', 1593), ('RO', 1594), ('RS', 1595),
        ('RU', 1596), ('RW', 1597), ('SA', 1598), ('SB', 1599),
        ('SC', 1600), ('SD', 1601), ('SE', 1602), ('SG', 1603),
        ('SH', 1604), ('SI', 1605), ('SJ', 1606), ('SK', 1607),
        ('SL', 1608), ('SM', 1609), ('SN', 1610), ('SO', 1611),
        ('SR', 1612), ('SS', 1613), ('ST', 1614), ('SV', 1615),
        ('SX', 1616), ('SY', 1617), ('SZ', 1618), ('TA', 1619),
        ('TC', 1620), ('TD', 1621), ('TF', 1622), ('TG', 1623),
        ('TH', 1624), ('TJ', 1625), ('TK', 1626), ('TL', 1627),
        ('TM', 1628), ('TN', 1629), ('TO', 1630), ('TR', 1631),
        ('TT', 1632), ('TV', 1633), ('TW', 1634), ('TZ', 1635),
        ('UA', 1636), ('UG', 1637), ('UM', 1638), ('UN', 1639),
        ('US', 1640), ('UY', 1641), ('UZ', 1642), ('VA', 1643),
        ('VC', 1644), ('VE', 1645), ('VG', 1646), ('VI', 1647),
        ('VN', 1648), ('VU', 1649), ('WF', 1650), ('WS', 1651),
        ('XK', 1652), ('YE', 1653), ('YT', 1654), ('ZA', 1655),
        ('ZM', 1656), ('ZW', 1657)
),
flag_badges AS (
    SELECT
        lower(code) AS sku,
        chr(127397 + ascii(substr(code, 1, 1)))
            || chr(127397 + ascii(substr(code, 2, 1))) AS emoji,
        sort_order
    FROM regional_flag_seed
),
subdivision_flag_badges AS (
    SELECT
        sku,
        '🏴'
            || (
                SELECT string_agg(chr(917504 + ascii(substr(tag, n, 1))), '' ORDER BY n)
                FROM generate_series(1, length(tag)) AS chars(n)
            )
            || chr(917631) AS emoji,
        sort_order
    FROM subdivision_flag_seed
),
all_flag_badges AS (
    SELECT sku, emoji, sort_order FROM extra_flag_seed
    UNION ALL
    SELECT sku, emoji, sort_order FROM subdivision_flag_badges
    UNION ALL
    SELECT sku, emoji, sort_order FROM flag_badges
)
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
SELECT
    'badge_flag_' || sku,
    'badge',
    'chat_badge',
    emoji,
    'Display ' || emoji || ' beside your chat name.',
    1000,
    jsonb_build_object('emoji', emoji, 'tier', 'basic', 'flag', sku),
    true,
    sort_order
FROM all_flag_badges
ON CONFLICT (sku) DO UPDATE SET
    item_kind = EXCLUDED.item_kind,
    slot = EXCLUDED.slot,
    name = EXCLUDED.name,
    description = EXCLUDED.description,
    price_chips = EXCLUDED.price_chips,
    payload = EXCLUDED.payload,
    active = EXCLUDED.active,
    sort_order = EXCLUDED.sort_order,
    updated = current_timestamp;
