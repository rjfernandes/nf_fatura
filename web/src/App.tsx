import { FiEye, FiEyeOff } from "react-icons/fi";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";
import BillingPage from "./pages/BillingPage";
import { isDesktop } from "./desktop";
import { toggleHideValues, useHideValues } from "./hideValues";
import CustomersPage from "./pages/CustomersPage";
import SettingsPage from "./pages/SettingsPage";
import StatementPage from "./pages/StatementPage";

const link = ({ isActive }: { isActive: boolean }) =>
  `rounded-md px-3 py-2 text-sm font-medium ${isActive ? "bg-indigo-600 text-white" : "text-slate-600 hover:bg-slate-200"}`;

export default function App() {
  const hideValues = useHideValues();
  return (
    <div className="mx-auto max-w-6xl p-4">
      <header className="mb-6 flex items-center gap-2 border-b border-slate-200 pb-3">
        <h1 className="mr-6 text-lg font-semibold">NF Fatura</h1>
        <NavLink to="/customers" className={link}>
          Clientes
        </NavLink>
        <NavLink to="/billing" className={link}>
          Faturamento
        </NavLink>
        <NavLink to="/statement" className={link}>
          Extrato
        </NavLink>
        {isDesktop && (
          <NavLink to="/settings" className={link}>
            Configurações
          </NavLink>
        )}
        <button
          type="button"
          onClick={toggleHideValues}
          title={hideValues ? "Mostrar valores" : "Esconder valores"}
          aria-label={hideValues ? "Mostrar valores" : "Esconder valores"}
          className="ml-auto rounded-md p-2 text-slate-500 hover:bg-slate-200 hover:text-slate-700"
        >
          {hideValues ? <FiEyeOff /> : <FiEye />}
        </button>
      </header>
      <Routes>
        <Route path="/customers" element={<CustomersPage />} />
        <Route path="/billing" element={<BillingPage />} />
        <Route path="/statement" element={<StatementPage />} />
        {isDesktop && <Route path="/settings" element={<SettingsPage />} />}
        <Route path="*" element={<Navigate to="/customers" replace />} />
      </Routes>
    </div>
  );
}
